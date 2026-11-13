use crate::media::Media;
use serde::{Deserialize, Serialize};
use std::{ffi::OsString, path::Path};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum Quality {
    High,
    #[default]
    Balanced,
    Small,
}
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum Codec {
    H264,
    #[default]
    H265,
    Av1,
}
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum Container {
    #[default]
    Auto,
    Mp4,
    Mkv,
}
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub quality: Quality,
    pub codec: Codec,
    pub container: Container,
    #[serde(default)]
    pub acceleration: Acceleration,
    #[serde(default)]
    pub parallelism: u8,
    #[serde(default)]
    pub goal: Goal,
    #[serde(default)]
    pub effort: Effort,
    #[serde(default)]
    pub recommendation: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Goal {
    Speed,
    #[default]
    Balanced,
    Quality,
    Size,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Effort {
    Speed,
    #[default]
    Balanced,
    Efficiency,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Acceleration {
    #[default]
    Auto,
    Cpu,
    Nvidia,
    Intel,
    Amd,
    Apple,
}

pub const DEMUXERS: &str = "mov,mp4,m4a,3gp,3g2,mj2,matroska,webm,avi,asf,flv,mpeg,mpegts,ogg";

pub fn output_extension(media: &Media, settings: &Settings) -> Result<&'static str, String> {
    if let Some(reason) = &media.blocked {
        return Err(reason.clone());
    }
    if media.hdr.is_some() && settings.codec != Codec::H265 {
        return Err("HDR 영상은 색 정보 보존을 위해 H.265를 선택해 주세요.".into());
    }
    let mkv = settings.container == Container::Mkv
        || (settings.container == Container::Auto && media.requires_mkv());
    if mkv
        && media
            .streams
            .iter()
            .any(|s| s.codec_type == "subtitle" && s.codec_name == "mov_text")
    {
        return Err("이 파일의 MP4 자막은 MKV에 그대로 담을 수 없습니다. MP4로 저장하거나 자막을 별도로 준비해 주세요.".into());
    }
    match settings.container {
        Container::Mp4 if media.requires_mkv() => {
            Err("원본 오디오·자막·첨부 보존을 위해 MKV 또는 자동을 선택해 주세요.".into())
        }
        Container::Mkv => Ok("mkv"),
        Container::Auto if media.requires_mkv() => Ok("mkv"),
        _ => Ok("mp4"),
    }
}

fn pair(args: &mut Vec<OsString>, key: &str, value: impl Into<OsString>) {
    args.push(key.into());
    args.push(value.into());
}

fn rational(value: &serde_json::Value, scale: f64) -> Option<u64> {
    let value = value.as_str()?;
    let (num, den) = value.split_once('/')?;
    let result = num.parse::<f64>().ok()? / den.parse::<f64>().ok()? * scale;
    (result.is_finite() && result >= 0.0).then(|| result.round() as u64)
}

fn hdr_params(media: &Media) -> Result<Vec<String>, String> {
    let mut params = vec![];
    if media.hdr.as_deref() == Some("HDR10") {
        params.push("hdr10=1".into());
    }
    for side in &media.video.side_data_list {
        match side["side_data_type"].as_str().unwrap_or("") {
            "Mastering display metadata" => {
                let read = |name: &str, scale| {
                    rational(&side[name], scale)
                        .ok_or_else(|| "HDR 마스터링 정보를 해석할 수 없습니다.".to_string())
                };
                params.push(format!(
                    "master-display=G({},{})B({},{})R({},{})WP({},{})L({},{})",
                    read("green_x", 50000.0)?,
                    read("green_y", 50000.0)?,
                    read("blue_x", 50000.0)?,
                    read("blue_y", 50000.0)?,
                    read("red_x", 50000.0)?,
                    read("red_y", 50000.0)?,
                    read("white_point_x", 50000.0)?,
                    read("white_point_y", 50000.0)?,
                    read("max_luminance", 10000.0)?,
                    read("min_luminance", 10000.0)?
                ));
            }
            "Content light level metadata" => {
                let content = side["max_content"]
                    .as_u64()
                    .ok_or("HDR 밝기 정보를 읽을 수 없습니다.")?;
                let average = side["max_average"]
                    .as_u64()
                    .ok_or("HDR 밝기 정보를 읽을 수 없습니다.")?;
                params.push(format!("max-cll={content},{average}"));
            }
            _ => {}
        }
    }
    Ok(params)
}

pub fn encode_args(
    input: &Path,
    output: &Path,
    media: &Media,
    settings: &Settings,
    sample: Option<(f64, f64)>,
) -> Result<Vec<OsString>, String> {
    encode_args_with_profile(
        input,
        output,
        media,
        settings,
        sample,
        crate::acceleration::EncodingProfile::cpu(crate::acceleration::cpu_threads()),
    )
}

pub fn encode_args_with_profile(
    input: &Path,
    output: &Path,
    media: &Media,
    settings: &Settings,
    sample: Option<(f64, f64)>,
    profile: crate::acceleration::EncodingProfile,
) -> Result<Vec<OsString>, String> {
    let extension = output_extension(media, settings)?;
    if !profile.supports(media) {
        return Err("이 영상의 색 정보와 비트 심도를 보존하려면 CPU 처리를 사용해 주세요.".into());
    }
    let mut args: Vec<OsString> = [
        "-hide_banner",
        "-loglevel",
        "error",
        "-nostdin",
        "-y",
        "-protocol_whitelist",
        "file,pipe",
        "-format_whitelist",
        DEMUXERS,
    ]
    .into_iter()
    .map(Into::into)
    .collect();
    if let Some((start, duration)) = sample {
        if !start.is_finite() || !duration.is_finite() || start < 0.0 || duration <= 0.0 {
            return Err("시험 압축 구간이 올바르지 않습니다.".into());
        }
        pair(&mut args, "-ss", format!("{start:.6}"));
    }
    pair(&mut args, "-threads", profile.threads.max(1).to_string());
    pair(
        &mut args,
        "-filter_threads",
        profile.threads.clamp(1, 4).to_string(),
    );
    pair(&mut args, "-i", input.as_os_str());
    let ordinal = if sample.is_some() {
        pair(&mut args, "-map", format!("0:{}", media.video.index));
        args.extend(["-an", "-sn", "-dn"].into_iter().map(Into::into));
        0
    } else {
        pair(&mut args, "-map", "0");
        pair(&mut args, "-c", "copy");
        media
            .streams
            .iter()
            .take_while(|s| s.index != media.video.index)
            .filter(|s| s.codec_type == "video")
            .count()
    };
    if let Some((_, duration)) = sample {
        pair(&mut args, "-t", format!("{duration:.6}"));
    }
    pair(&mut args, "-map_metadata", "0");
    pair(
        &mut args,
        "-map_chapters",
        if sample.is_some() { "-1" } else { "0" },
    );
    pair(&mut args, "-c:a", "copy");
    args.extend(profile.encoder_args_with_effort(
        settings.codec,
        settings.quality,
        ordinal,
        settings.effort,
    )?);
    pair(&mut args, &format!("-fps_mode:v:{ordinal}"), "passthrough");
    let ten_bit = media.hdr.is_some()
        || media.video.pix_fmt.contains("10")
        || media.video.pix_fmt.contains("12");
    let pixel = profile.pixel_format(ten_bit, settings.codec);
    pair(&mut args, &format!("-pix_fmt:v:{ordinal}"), pixel);
    pair(
        &mut args,
        &format!("-filter:v:{ordinal}"),
        "pad=ceil(iw/2)*2:ceil(ih/2)*2",
    );
    for (key, value) in [
        ("color_primaries", &media.video.color_primaries),
        ("color_trc", &media.video.color_transfer),
        ("colorspace", &media.video.color_space),
    ] {
        if !value.is_empty() && value != "unknown" {
            pair(&mut args, &format!("-{key}:v:{ordinal}"), value.as_str());
        }
    }
    if settings.codec == Codec::H265 && profile.backend == crate::acceleration::Backend::Cpu {
        let mut params = vec![
            "log-level=error".to_string(),
            format!("pools={}", profile.threads.max(1)),
            format!("frame-threads={}", profile.threads.clamp(1, 4)),
        ];
        params.extend(hdr_params(media)?);
        pair(
            &mut args,
            &format!("-x265-params:v:{ordinal}"),
            params.join(":"),
        );
    }
    if sample.is_none() && extension == "mp4" {
        pair(&mut args, "-movflags", "+faststart");
        if settings.codec == Codec::H265 {
            pair(&mut args, &format!("-tag:v:{ordinal}"), "hvc1");
        }
    }
    pair(&mut args, "-progress", "pipe:1");
    args.push("-nostats".into());
    pair(
        &mut args,
        "-f",
        if sample.is_some() || extension == "mkv" {
            "matroska"
        } else {
            "mp4"
        },
    );
    args.push(output.as_os_str().into());
    Ok(args)
}
