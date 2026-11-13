use crate::{
    command::{Acceleration, Codec, Effort, Quality, Settings},
    engine::Tools,
    media::Media,
    process::{execute, ProcessError},
};
use serde::Serialize;
use std::{
    ffi::OsString,
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Backend {
    Cpu,
    Nvidia,
    Intel,
    Amd,
    Apple,
}

impl Backend {
    pub fn label(self) -> &'static str {
        match self {
            Self::Cpu => "CPU",
            Self::Nvidia => "GPU · NVIDIA",
            Self::Intel => "GPU · Intel",
            Self::Amd => "GPU · AMD",
            Self::Apple => "GPU · Apple",
        }
    }
    pub fn encoder(self, codec: Codec) -> Option<&'static str> {
        match (self, codec) {
            (Self::Cpu, Codec::H264) => Some("libx264"),
            (Self::Cpu, Codec::H265) => Some("libx265"),
            (Self::Cpu, Codec::Av1) => Some("libsvtav1"),
            (Self::Nvidia, Codec::H264) => Some("h264_nvenc"),
            (Self::Nvidia, Codec::H265) => Some("hevc_nvenc"),
            (Self::Nvidia, Codec::Av1) => Some("av1_nvenc"),
            (Self::Intel, Codec::H264) => Some("h264_qsv"),
            (Self::Intel, Codec::H265) => Some("hevc_qsv"),
            (Self::Intel, Codec::Av1) => Some("av1_qsv"),
            (Self::Amd, Codec::H264) => Some("h264_amf"),
            (Self::Amd, Codec::H265) => Some("hevc_amf"),
            (Self::Amd, Codec::Av1) => Some("av1_amf"),
            (Self::Apple, Codec::H264) => Some("h264_videotoolbox"),
            (Self::Apple, Codec::H265) => Some("hevc_videotoolbox"),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HardwareEncoder {
    pub codec: Codec,
    pub backend: Backend,
}

#[derive(Debug, Clone)]
pub struct Capabilities {
    pub codecs: Vec<Codec>,
    pub hardware: Vec<HardwareEncoder>,
    pub cpu_threads: usize,
}

impl Capabilities {
    pub fn cpu_reason(&self, media: &Media, settings: &Settings) -> Option<&'static str> {
        if settings.acceleration == Acceleration::Cpu {
            None
        } else if !gpu_safe(media) {
            Some("영상의 색 정보를 보존하기 위해 CPU로 처리합니다.")
        } else {
            None
        }
    }
    pub fn detect(tools: &Tools) -> Result<Self, String> {
        let cancel = AtomicBool::new(false);
        let output = execute(
            &tools.ffmpeg,
            &["-hide_banner".into(), "-encoders".into()],
            &cancel,
            Duration::from_secs(15),
            true,
            &mut |_| {},
        )?;
        let text = String::from_utf8_lossy(&output);
        let listed = |name: &str| text.split_whitespace().any(|word| word == name);
        let codecs: Vec<_> = [Codec::H264, Codec::H265, Codec::Av1]
            .into_iter()
            .filter(|&codec| listed(Backend::Cpu.encoder(codec).unwrap()))
            .collect();
        if codecs.is_empty() {
            return Err("지원하는 영상 인코더가 없습니다. FFmpeg 구성을 확인해 주세요.".into());
        }
        let mut hardware = Vec::new();
        let started = Instant::now();
        for &codec in &codecs {
            for backend in [
                Backend::Nvidia,
                Backend::Intel,
                Backend::Amd,
                Backend::Apple,
            ] {
                let Some(encoder) = backend.encoder(codec).filter(|name| listed(name)) else {
                    continue;
                };
                let remaining = Duration::from_secs(12).saturating_sub(started.elapsed());
                if remaining.is_zero() {
                    break;
                }
                let profile = EncodingProfile {
                    backend,
                    threads: 1,
                };
                let mut args: Vec<OsString> = [
                    "-hide_banner",
                    "-loglevel",
                    "error",
                    "-nostdin",
                    "-f",
                    "lavfi",
                    "-i",
                    "color=size=256x144:rate=30",
                    "-frames:v",
                    "3",
                    "-an",
                ]
                .into_iter()
                .map(Into::into)
                .collect();
                args.extend(profile.encoder_args(codec, Quality::Balanced, 0)?);
                args.extend(
                    [
                        "-pix_fmt",
                        profile.pixel_format(false, codec),
                        "-f",
                        "null",
                        "-",
                    ]
                    .into_iter()
                    .map(Into::into),
                );
                // Listing an encoder does not prove a usable device/driver exists.
                if execute(
                    &tools.ffmpeg,
                    &args,
                    &cancel,
                    remaining.min(Duration::from_secs(3)),
                    false,
                    &mut |_| {},
                )
                .is_ok()
                {
                    debug_assert_eq!(profile.backend.encoder(codec), Some(encoder));
                    hardware.push(HardwareEncoder { codec, backend });
                }
            }
        }
        Ok(Self {
            codecs,
            hardware,
            cpu_threads: cpu_threads(),
        })
    }

    pub fn profile(&self, media: &Media, settings: &Settings, threads: usize) -> EncodingProfile {
        let backend = if settings.acceleration != Acceleration::Cpu && gpu_safe(media) {
            self.hardware
                .iter()
                .find(|entry| {
                    entry.codec == settings.codec && accepts(settings.acceleration, entry.backend)
                })
                .map(|entry| entry.backend)
                .unwrap_or(Backend::Cpu)
        } else {
            Backend::Cpu
        };
        EncodingProfile {
            backend,
            threads: threads.max(1),
        }
    }
}

pub fn accepts(acceleration: Acceleration, backend: Backend) -> bool {
    match acceleration {
        Acceleration::Auto => true,
        Acceleration::Cpu => backend == Backend::Cpu,
        Acceleration::Nvidia => backend == Backend::Nvidia,
        Acceleration::Intel => backend == Backend::Intel,
        Acceleration::Amd => backend == Backend::Amd,
        Acceleration::Apple => backend == Backend::Apple,
    }
}

pub fn cpu_threads() -> usize {
    std::thread::available_parallelism()
        .map(usize::from)
        .unwrap_or(1)
}

pub fn run_with_cpu_fallback<T>(
    profile: EncodingProfile,
    cancel: &AtomicBool,
    mut attempt: impl FnMut(EncodingProfile, bool) -> Result<T, ProcessError>,
) -> Result<T, ProcessError> {
    match attempt(profile, false) {
        Err(error) if profile.backend != Backend::Cpu && !cancel.load(std::sync::atomic::Ordering::Acquire) && error.hardware_initialization_failed() => {
            attempt(EncodingProfile::cpu(profile.threads), true).map_err(|cpu| ProcessError::Message(format!("GPU 가속을 시작하지 못했습니다: {error}\nCPU 재시도도 완료하지 못했습니다: {cpu}")))
        }
        result => result,
    }
}

fn gpu_safe(media: &Media) -> bool {
    media.hdr.is_none()
        && matches!(
            media.video.pix_fmt.as_str(),
            "yuv420p"
                | "yuv422p"
                | "yuv444p"
                | "yuvj420p"
                | "yuvj422p"
                | "yuvj444p"
                | "nv12"
                | "nv21"
                | "rgb24"
                | "bgr24"
                | "gbrp"
                | "gray"
                | "yuyv422"
                | "uyvy422"
        )
}

#[derive(Debug, Clone, Copy)]
pub struct EncodingProfile {
    pub backend: Backend,
    pub threads: usize,
}

impl EncodingProfile {
    pub fn cpu(threads: usize) -> Self {
        Self {
            backend: Backend::Cpu,
            threads: threads.max(1),
        }
    }
    pub fn supports(self, media: &Media) -> bool {
        self.backend == Backend::Cpu || gpu_safe(media)
    }
    pub fn pixel_format(self, ten_bit: bool, codec: Codec) -> &'static str {
        if self.backend == Backend::Cpu && ten_bit && codec != Codec::H264 {
            "yuv420p10le"
        } else if matches!(self.backend, Backend::Intel | Backend::Amd | Backend::Apple) {
            "nv12"
        } else {
            "yuv420p"
        }
    }
    pub fn encoder_args(
        self,
        codec: Codec,
        quality: Quality,
        ordinal: usize,
    ) -> Result<Vec<OsString>, String> {
        self.encoder_args_with_effort(codec, quality, ordinal, Effort::Balanced)
    }
    pub fn encoder_args_with_effort(
        self,
        codec: Codec,
        quality: Quality,
        ordinal: usize,
        effort: Effort,
    ) -> Result<Vec<OsString>, String> {
        let encoder = self
            .backend
            .encoder(codec)
            .ok_or("선택한 GPU가 이 압축 방식을 지원하지 않습니다.")?;
        let mut args = vec![];
        let mut pair = |name: &str, value: String| {
            args.push(format!("-{name}:v:{ordinal}").into());
            args.push(OsString::from(value));
        };
        pair("c", encoder.into());
        let q = match quality {
            Quality::High => 0,
            Quality::Balanced => 1,
            Quality::Small => 2,
        };
        match self.backend {
            Backend::Cpu => {
                pair(
                    "threads",
                    if codec == Codec::H265 {
                        self.threads.min(4)
                    } else {
                        self.threads
                    }
                    .to_string(),
                );
                pair(
                    "preset",
                    match (codec, effort) {
                        (Codec::Av1, Effort::Speed) => "10",
                        (Codec::Av1, Effort::Efficiency) => "2",
                        (Codec::Av1, _) => "6",
                        (_, Effort::Speed) => "ultrafast",
                        (_, Effort::Efficiency) => "veryslow",
                        _ => "fast",
                    }
                    .into(),
                );
                pair(
                    "crf",
                    if codec == Codec::Av1 {
                        [24, 30, 36][q]
                    } else {
                        [20, 23, 27][q]
                    }
                    .to_string(),
                );
                if codec == Codec::Av1 {
                    // SVT >=3 uses parallelism levels, not a logical-processor count.
                    let level = match self.threads {
                        0..=1 => 1,
                        2 => 2,
                        3..=5 => 3,
                        6..=11 => 4,
                        12..=23 => 5,
                        _ => 6,
                    };
                    pair("svtav1-params", format!("lp={level}"));
                }
            }
            Backend::Nvidia => {
                pair(
                    "preset",
                    match effort {
                        Effort::Speed => "p1",
                        Effort::Balanced => "p5",
                        Effort::Efficiency => "p7",
                    }
                    .into(),
                );
                pair("tune", "hq".into());
                pair("rc", "vbr".into());
                // Hardware CQ and software CRF are independent quality scales.
                pair(
                    "cq",
                    if codec == Codec::Av1 {
                        [25, 29, 34][q]
                    } else {
                        [19, 22, 26][q]
                    }
                    .to_string(),
                );
                pair("b", "0".into());
            }
            Backend::Intel => {
                pair(
                    "preset",
                    match effort {
                        Effort::Speed => "veryfast",
                        Effort::Balanced => "medium",
                        Effort::Efficiency => "veryslow",
                    }
                    .into(),
                );
                pair("global_quality", [20, 23, 27][q].to_string());
                if codec != Codec::Av1 {
                    pair("look_ahead", "0".into());
                }
            }
            Backend::Amd => {
                pair(
                    "quality",
                    match effort {
                        Effort::Speed => "speed",
                        Effort::Balanced => "balanced",
                        Effort::Efficiency => "quality",
                    }
                    .into(),
                );
                pair("rc", "cqp".into());
                pair(
                    "qp_i",
                    if codec == Codec::Av1 {
                        [64, 92, 120][q]
                    } else {
                        [18, 21, 25][q]
                    }
                    .to_string(),
                );
                pair(
                    "qp_p",
                    if codec == Codec::Av1 {
                        [72, 100, 128][q]
                    } else {
                        [20, 23, 27][q]
                    }
                    .to_string(),
                );
            }
            Backend::Apple => {
                pair("allow_sw", "0".into());
                if effort != Effort::Balanced {
                    pair(
                        "prio_speed",
                        if effort == Effort::Speed { "1" } else { "0" }.into(),
                    );
                }
                pair("q", [75, 65, 55][q].to_string());
                pair("b", "0".into());
            }
        }
        Ok(args)
    }
}

#[derive(Debug)]
pub struct WorkBudget {
    pub workers: usize,
    pub threads_per_worker: usize,
}

impl WorkBudget {
    pub fn new(cpu_threads: usize, files: usize, settings: &Settings) -> Result<Self, String> {
        if !matches!(settings.parallelism, 0 | 1 | 2 | 4) {
            return Err("동시 작업 수는 자동, 1개, 2개, 4개 중에서 선택해 주세요.".into());
        }
        let cores = cpu_threads.max(1);
        let requested = if settings.parallelism == 0 {
            if cores >= 4 {
                2
            } else {
                1
            }
        } else {
            usize::from(settings.parallelism)
        };
        let workers = requested.min(cores).min(files);
        Ok(Self {
            workers,
            threads_per_worker: (cores / workers.max(1)).max(1),
        })
    }
}
