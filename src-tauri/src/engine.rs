use crate::{
    acceleration::{cpu_threads, EncodingProfile},
    command::{encode_args_with_profile, output_extension, Settings, DEMUXERS},
    media::Media,
    policy::{estimate_size, sample_windows},
    process::{execute, execute_checked, ProcessError},
};
use serde::Serialize;
use std::{
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

pub fn canonical_input(path: &Path) -> Result<PathBuf, String> {
    let path = path
        .canonicalize()
        .map_err(|_| "파일을 찾을 수 없습니다. 이동하거나 삭제했는지 확인해 주세요.")?;
    let meta = fs::metadata(&path).map_err(|e| e.to_string())?;
    if !meta.is_file() || meta.len() == 0 {
        return Err("비어 있지 않은 로컬 영상 파일을 선택해 주세요.".into());
    }
    Ok(path)
}

#[derive(Clone)]
pub struct Tools {
    pub ffmpeg: PathBuf,
    pub ffprobe: PathBuf,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompressionResult {
    pub output_path: Option<String>,
    pub output_bytes: u64,
    pub saved: bool,
}

impl Tools {
    pub fn system() -> Result<Self, String> {
        let find = |name: &str| {
            std::env::var_os("PATH")
                .and_then(|paths| {
                    std::env::split_paths(&paths)
                        .map(|p| p.join(format!("{name}{}", std::env::consts::EXE_SUFFIX)))
                        .find(|p| p.is_file())
                })
                .ok_or_else(|| format!("{name} 실행 파일을 찾을 수 없습니다."))
        };
        Ok(Self {
            ffmpeg: find("ffmpeg")?,
            ffprobe: find("ffprobe")?,
        })
    }
    pub fn probe(&self, path: &Path, cancel: &AtomicBool) -> Result<Media, String> {
        let path = canonical_input(path)?;
        let mut args: Vec<OsString> = [
            "-v",
            "error",
            "-protocol_whitelist",
            "file,pipe",
            "-format_whitelist",
            DEMUXERS,
            "-show_streams",
            "-show_format",
            "-of",
            "json",
        ]
        .into_iter()
        .map(Into::into)
        .collect();
        args.push(path.as_os_str().into());
        let output = execute(
            &self.ffprobe,
            &args,
            cancel,
            Duration::from_secs(45),
            true,
            &mut |_| {},
        )?;
        let mut value: serde_json::Value =
            serde_json::from_slice(&output).map_err(|_| "영상 정보를 해석할 수 없습니다.")?;
        // Static HDR metadata can be on the first decoded frame, not on the stream.
        if let Some(index) = value["streams"].as_array().and_then(|s| {
            s.iter()
                .position(|v| v["codec_type"] == "video" && v["disposition"]["attached_pic"] != 1)
        }) {
            let stream = &value["streams"][index];
            if stream["codec_name"] == "hevc"
                || matches!(
                    stream["color_transfer"].as_str(),
                    Some("smpte2084" | "arib-std-b67")
                )
            {
                let mut frame_args: Vec<OsString> = [
                    "-v",
                    "error",
                    "-protocol_whitelist",
                    "file,pipe",
                    "-format_whitelist",
                    DEMUXERS,
                    "-select_streams",
                    "v:0",
                    "-read_intervals",
                    "%+#1",
                    "-show_frames",
                    "-show_entries",
                    "frame=side_data_list",
                    "-of",
                    "json",
                ]
                .into_iter()
                .map(Into::into)
                .collect();
                frame_args.push(path.as_os_str().into());
                let frame = execute(
                    &self.ffprobe,
                    &frame_args,
                    cancel,
                    Duration::from_secs(45),
                    true,
                    &mut |_| {},
                )?;
                let frame: serde_json::Value = serde_json::from_slice(&frame)
                    .map_err(|_| "HDR 프레임 정보를 해석할 수 없습니다.")?;
                let mut side = value["streams"][index]["side_data_list"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default();
                if let Some(data) = frame["frames"][0]["side_data_list"].as_array() {
                    for entry in data {
                        if !side.contains(entry) {
                            side.push(entry.clone());
                        }
                    }
                }
                value["streams"][index]["side_data_list"] = side.into();
            }
        }
        Media::from_probe(value, fs::metadata(path).map_err(|e| e.to_string())?.len())
    }
    pub fn estimate(
        &self,
        path: &Path,
        media: &Media,
        settings: &Settings,
        cancel: &AtomicBool,
        progress: &mut dyn FnMut(f64),
    ) -> Result<u64, String> {
        self.estimate_with_profile(
            path,
            media,
            settings,
            cancel,
            progress,
            EncodingProfile::cpu(cpu_threads()),
        )
        .map_err(|error| error.to_string())
    }
    pub fn estimate_with_profile(
        &self,
        path: &Path,
        media: &Media,
        settings: &Settings,
        cancel: &AtomicBool,
        progress: &mut dyn FnMut(f64),
        profile: EncodingProfile,
    ) -> Result<u64, ProcessError> {
        output_extension(media, settings)?;
        let audio = match media.audio_bps() {
            Some(rate) => rate,
            None => self.sample_audio_bps(path, media, cancel)?,
        };
        let temp =
            tempfile::tempdir().map_err(|e| format!("시험 압축 공간을 만들지 못했습니다: {e}"))?;
        let windows = sample_windows(media.duration);
        let mut samples = vec![];
        for (index, (start, length)) in windows.iter().copied().enumerate() {
            let sample = temp.path().join(format!("sample-{index}.mkv"));
            let args = encode_args_with_profile(
                path,
                &sample,
                media,
                settings,
                Some((start, length)),
                profile,
            )?;
            execute_checked(
                &self.ffmpeg,
                &args,
                cancel,
                Duration::from_secs(300),
                false,
                &mut |seconds| {
                    progress(
                        (index as f64 + (seconds / length).clamp(0.0, 1.0)) / windows.len() as f64,
                    );
                },
            )?;
            let actual = self.probe(&sample, cancel)?;
            samples.push((actual.duration, actual.bytes));
        }
        estimate_size(&samples, media.duration, audio)
            .ok_or_else(|| "이 파일의 예상 용량을 계산할 수 없습니다.".into())
    }
    fn sample_audio_bps(
        &self,
        path: &Path,
        media: &Media,
        cancel: &AtomicBool,
    ) -> Result<u64, String> {
        let mut bytes = 0_u64;
        let mut seconds = 0.0;
        for (start, length) in sample_windows(media.duration) {
            let mut args: Vec<OsString> = [
                "-v",
                "error",
                "-protocol_whitelist",
                "file,pipe",
                "-format_whitelist",
                DEMUXERS,
                "-select_streams",
                "a",
                "-show_packets",
                "-show_entries",
                "packet=pts_time,dts_time,size",
                "-of",
                "json",
                "-read_intervals",
            ]
            .into_iter()
            .map(Into::into)
            .collect();
            args.push(format!("{start:.6}%+{length:.6}").into());
            args.push(path.as_os_str().into());
            let output = execute(
                &self.ffprobe,
                &args,
                cancel,
                Duration::from_secs(45),
                true,
                &mut |_| {},
            )?;
            let value: serde_json::Value = serde_json::from_slice(&output)
                .map_err(|_| "오디오 시험 정보를 해석할 수 없습니다.")?;
            for packet in value["packets"]
                .as_array()
                .ok_or("오디오 패킷을 확인할 수 없습니다.")?
            {
                let time = packet["pts_time"]
                    .as_str()
                    .or_else(|| packet["dts_time"].as_str())
                    .and_then(|s| s.parse::<f64>().ok())
                    .filter(|t| t.is_finite())
                    .ok_or("오디오 재생 시점을 확인할 수 없어 예측할 수 없습니다.")?;
                if time >= start && time < start + length {
                    let size = packet["size"]
                        .as_str()
                        .and_then(|s| s.parse::<u64>().ok())
                        .ok_or("오디오 패킷 크기를 확인할 수 없습니다.")?;
                    bytes = bytes
                        .checked_add(size)
                        .ok_or("오디오 크기가 너무 큽니다.")?;
                }
            }
            seconds += length;
        }
        Ok((bytes as f64 * 8.0 / seconds).ceil() as u64)
    }
    pub fn compress(
        &self,
        path: &Path,
        media: &Media,
        settings: &Settings,
        output_dir: &Path,
        cancel: &AtomicBool,
        progress: &mut dyn FnMut(f64),
    ) -> Result<CompressionResult, String> {
        self.compress_with_profile(
            path,
            media,
            settings,
            output_dir,
            cancel,
            progress,
            EncodingProfile::cpu(cpu_threads()),
        )
        .map_err(|error| error.to_string())
    }
    pub fn compress_with_profile(
        &self,
        path: &Path,
        media: &Media,
        settings: &Settings,
        output_dir: &Path,
        cancel: &AtomicBool,
        progress: &mut dyn FnMut(f64),
        profile: EncodingProfile,
    ) -> Result<CompressionResult, ProcessError> {
        if cancel.load(Ordering::Acquire) {
            return Err("작업이 취소되었습니다.".into());
        }
        let path = canonical_input(path)?;
        let before = fs::metadata(&path).map_err(|e| e.to_string())?;
        if before.len() != media.bytes {
            return Err("추가한 뒤 원본 파일이 변경되었습니다. 파일을 다시 추가해 주세요.".into());
        }
        let extension = output_extension(media, settings)?;
        if fs::symlink_metadata(output_dir).is_ok_and(|m| m.file_type().is_symlink()) {
            return Err("바로가기·심볼릭 링크 대신 실제 저장 폴더를 선택해 주세요.".into());
        }
        fs::create_dir_all(output_dir)
            .map_err(|e| format!("저장 폴더를 만들지 못했습니다: {e}"))?;
        let output_dir = output_dir.canonicalize().map_err(|e| e.to_string())?;
        let temp = tempfile::Builder::new()
            .prefix(".dropper-")
            .suffix(&format!(".{extension}"))
            .tempfile_in(&output_dir)
            .map_err(|e| format!("저장 파일을 만들지 못했습니다: {e}"))?;
        let args = encode_args_with_profile(&path, temp.path(), media, settings, None, profile)?;
        execute_checked(
            &self.ffmpeg,
            &args,
            cancel,
            Duration::from_secs(24 * 60 * 60),
            false,
            &mut |seconds| progress((seconds / media.duration).clamp(0.0, 0.99)),
        )?;
        let output = self.probe(temp.path(), cancel)?;
        validate_output(media, &output)?;
        let after =
            fs::metadata(&path).map_err(|_| "압축 중 원본 파일이 이동하거나 삭제되었습니다.")?;
        if before.len() != after.len() || before.modified().ok() != after.modified().ok() {
            return Err("압축 중 원본이 변경되어 결과를 저장하지 않았습니다.".into());
        }
        if cancel.load(Ordering::Acquire) {
            return Err("작업이 취소되었습니다.".into());
        }
        if output.bytes >= media.bytes {
            return Ok(CompressionResult {
                output_path: None,
                output_bytes: output.bytes,
                saved: false,
            });
        }
        let stem = path
            .file_stem()
            .ok_or("원본 파일 이름을 확인할 수 없습니다.")?
            .to_string_lossy();
        let mut pending = temp;
        for count in 0..10_000 {
            let suffix = if count == 0 {
                String::new()
            } else {
                format!("_{count}")
            };
            let destination = output_dir.join(format!("{stem}_compressed{suffix}.{extension}"));
            if destination == path {
                continue;
            }
            match pending.persist_noclobber(&destination) {
                Ok(file) => {
                    file.sync_all()
                        .map_err(|e| format!("저장 확인에 실패했습니다: {e}"))?;
                    progress(1.0);
                    return Ok(CompressionResult {
                        output_path: Some(destination.to_string_lossy().into()),
                        output_bytes: output.bytes,
                        saved: true,
                    });
                }
                Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists => {
                    pending = error.file;
                }
                Err(error) => {
                    return Err(format!("압축 파일을 저장하지 못했습니다: {}", error.error).into())
                }
            }
        }
        Err("같은 이름의 파일이 너무 많습니다. 다른 저장 폴더를 선택해 주세요.".into())
    }
}

fn validate_output(source: &Media, output: &Media) -> Result<(), String> {
    if (output.duration - source.duration).abs() > 0.5_f64.max(source.duration * 0.01) {
        return Err("결과 영상의 재생 시간이 원본과 달라 저장하지 않았습니다.".into());
    }
    for kind in ["video", "audio", "subtitle", "attachment"] {
        if source
            .streams
            .iter()
            .filter(|s| s.codec_type == kind)
            .count()
            != output
                .streams
                .iter()
                .filter(|s| s.codec_type == kind)
                .count()
        {
            return Err(format!(
                "{kind} 트랙 보존을 확인하지 못해 저장하지 않았습니다."
            ));
        }
    }
    if source.hdr.is_some()
        && (source.hdr != output.hdr
            || !output.video.pix_fmt.contains("10")
            || source.video.color_primaries != output.video.color_primaries
            || source.video.color_space != output.video.color_space)
    {
        return Err("HDR 색 정보 보존을 확인하지 못해 저장하지 않았습니다.".into());
    }
    if source.hdr.is_some() {
        for side in &source.video.side_data_list {
            if matches!(
                side["side_data_type"].as_str(),
                Some("Mastering display metadata" | "Content light level metadata")
            ) && !output.video.side_data_list.contains(side)
            {
                return Err("HDR 정적 메타데이터 보존을 확인하지 못해 저장하지 않았습니다.".into());
            }
        }
    }
    Ok(())
}
