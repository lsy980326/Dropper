//! Bounded, per-file measurements. Scores compare tested candidates, not vendors.
use crate::{
    acceleration::{accepts, Backend, Capabilities, EncodingProfile},
    command::{
        encode_args_with_profile, output_extension, Codec, Effort, Goal, Quality, Settings,
        DEMUXERS,
    },
    engine::Tools,
    media::Media,
    process::{execute_checked, ProcessError},
};
use std::{
    ffi::OsString,
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

#[derive(Clone, Debug)]
pub struct Choice {
    pub settings: Settings,
    pub profile: EncodingProfile,
}
#[derive(Clone, Debug)]
pub struct Measurement {
    pub choice: Choice,
    pub bytes_per_frame: f64,
    pub seconds_per_frame: f64,
    pub similarity: f64,
}

pub fn candidates(
    caps: &Capabilities,
    media: &Media,
    settings: &Settings,
    threads: usize,
) -> Vec<Choice> {
    let mut result = Vec::new();
    for codec in [settings.codec, Codec::H264, Codec::H265, Codec::Av1] {
        if !caps.codecs.contains(&codec)
            || result.iter().any(|c: &Choice| c.settings.codec == codec)
        {
            continue;
        }
        for backend in [
            Backend::Cpu,
            Backend::Nvidia,
            Backend::Intel,
            Backend::Amd,
            Backend::Apple,
        ] {
            if !accepts(settings.acceleration, backend)
                || (backend != Backend::Cpu
                    && !caps
                        .hardware
                        .iter()
                        .any(|h| h.codec == codec && h.backend == backend))
            {
                continue;
            }
            let profile = EncodingProfile { backend, threads };
            let chosen = Settings {
                codec,
                recommendation: false,
                ..settings.clone()
            };
            if profile.supports(media) && output_extension(media, &chosen).is_ok() {
                result.push(Choice {
                    settings: chosen,
                    profile,
                });
            }
        }
    }
    result
}

pub fn choose(results: &[Measurement], goal: Goal) -> Option<usize> {
    let threshold = match goal {
        Goal::Quality => 0.98,
        Goal::Size => 0.94,
        _ => 0.96,
    };
    results
        .iter()
        .enumerate()
        .filter(|(_, r)| {
            r.similarity.is_finite()
                && r.similarity >= threshold
                && r.similarity <= 1.0
                && r.bytes_per_frame.is_finite()
                && r.bytes_per_frame > 0.0
                && r.seconds_per_frame.is_finite()
                && r.seconds_per_frame > 0.0
        })
        .min_by(|(_, a), (_, b)| {
            let score = |r: &Measurement| match goal {
                Goal::Speed => r.seconds_per_frame,
                Goal::Size => r.bytes_per_frame,
                Goal::Quality => -r.similarity,
                Goal::Balanced => r.seconds_per_frame.sqrt() * r.bytes_per_frame.sqrt(),
            };
            score(a).total_cmp(&score(b))
        })
        .map(|(i, _)| i)
}

pub struct Recommendation {
    pub choice: Choice,
    pub note: String,
}

pub fn recommend(
    tools: &Tools,
    path: &Path,
    media: &Media,
    settings: &Settings,
    caps: &Capabilities,
    threads: usize,
    cancel: &AtomicBool,
) -> Result<Recommendation, ProcessError> {
    if cancel.load(Ordering::Acquire) {
        return Err("작업이 취소되었습니다.".into());
    }
    let mut baseline = Settings {
        recommendation: false,
        quality: match settings.goal {
            Goal::Quality => Quality::High,
            Goal::Size => Quality::Small,
            _ => Quality::Balanced,
        },
        effort: match settings.goal {
            Goal::Speed => Effort::Speed,
            Goal::Size => Effort::Efficiency,
            _ => Effort::Balanced,
        },
        ..settings.clone()
    };
    if media.hdr.is_some()
        || media.video.pix_fmt.contains("10")
        || media.video.pix_fmt.contains("12")
    {
        baseline.codec = Codec::H265;
    }
    if !caps.codecs.contains(&baseline.codec) {
        if media.hdr.is_some() {
            return Err("HDR 보존에 필요한 H.265 인코더를 사용할 수 없습니다.".into());
        }
        baseline.codec = *caps
            .codecs
            .first()
            .ok_or("사용할 수 있는 영상 인코더가 없습니다.")?;
    }
    output_extension(media, &baseline)?;
    let fallback = Choice {
        profile: caps.profile(media, &baseline, threads),
        settings: baseline,
    };
    if media.duration < 30.0
        || media.hdr.is_some()
        || media.video.pix_fmt.contains("10")
        || media.video.pix_fmt.contains("12")
    {
        return Ok(Recommendation{choice:fallback,note:"자동 추천 · 짧은 영상 또는 고비트 심도 영상은 시험 대신 선택한 기본 설정을 유지합니다.".into()});
    }
    let start = Instant::now();
    let deadline = Duration::from_secs(18);
    let mut results = Vec::new();
    for choice in candidates(caps, media, &fallback.settings, threads) {
        if cancel.load(Ordering::Acquire) {
            return Err("작업이 취소되었습니다.".into());
        }
        let remaining = deadline.saturating_sub(start.elapsed());
        if remaining < Duration::from_millis(500) {
            break;
        }
        // One deadline covers encoding and comparison, so slow presets cannot stall a batch.
        let budget = remaining.min(Duration::from_secs(4));
        let trial = Instant::now();
        let temp = tempfile::Builder::new()
            .suffix(".mkv")
            .tempfile()
            .map_err(|e| ProcessError::Message(format!("시험 파일을 만들지 못했습니다: {e}")))?;
        let seek = media.duration * 0.5;
        let mut args = encode_args_with_profile(
            path,
            temp.path(),
            media,
            &choice.settings,
            Some((seek, 4.0)),
            choice.profile,
        )?;
        let output = args.pop().ok_or("시험 출력 경로가 없습니다.")?;
        args.extend([OsString::from("-frames:v:0"), OsString::from("48"), output]);
        match execute_checked(&tools.ffmpeg, &args, cancel, budget, false, &mut |_| {}) {
            Ok(_) => (),
            Err(error @ ProcessError::Exit { .. }) if fatal_trial_error(&error) => {
                return Err(error)
            }
            Err(ProcessError::Exit { .. } | ProcessError::TimedOut) => continue,
            Err(error) => return Err(error),
        }
        let encode_time = trial.elapsed().as_secs_f64();
        let left = budget.saturating_sub(trial.elapsed());
        if left.is_zero() {
            continue;
        }
        let filter=format!("[0:{}]pad=ceil(iw/2)*2:ceil(ih/2)*2,format=yuv420p,settb=AVTB,setpts=N[a];[1:v:0]format=yuv420p,settb=AVTB,setpts=N[b];[a][b]ssim=stats_file=-:shortest=1:repeatlast=0",media.video.index);
        let mut compare: Vec<OsString> = [
            "-hide_banner",
            "-loglevel",
            "error",
            "-nostdin",
            "-threads",
            "1",
            "-filter_complex_threads",
            "1",
            "-protocol_whitelist",
            "file,pipe",
            "-format_whitelist",
            DEMUXERS,
            "-ss",
        ]
        .into_iter()
        .map(Into::into)
        .collect();
        compare.extend([
            seek.to_string().into(),
            "-t".into(),
            "4".into(),
            "-i".into(),
            path.as_os_str().to_owned(),
            "-i".into(),
            temp.path().as_os_str().to_owned(),
            "-filter_complex".into(),
            filter.into(),
            "-frames:v".into(),
            "48".into(),
            "-an".into(),
            "-f".into(),
            "null".into(),
            "-".into(),
        ]);
        let output = match execute_checked(&tools.ffmpeg, &compare, cancel, left, true, &mut |_| {})
        {
            Ok(output) => output,
            Err(error @ ProcessError::Exit { .. }) if fatal_trial_error(&error) => {
                return Err(error)
            }
            Err(ProcessError::Exit { .. } | ProcessError::TimedOut) => continue,
            Err(error) => return Err(error),
        };
        let scores: Vec<f64> = String::from_utf8_lossy(&output)
            .lines()
            .filter_map(|line| {
                line.split_whitespace()
                    .find_map(|part| part.strip_prefix("All:")?.parse().ok())
            })
            .collect();
        // Reject truncation and repeated-tail comparisons; all 48 source frames must exist.
        if scores.len() != 48 || scores.iter().any(|s| !s.is_finite()) {
            continue;
        }
        let bytes = temp.as_file().metadata().map_err(|e| e.to_string())?.len();
        results.push(Measurement {
            choice,
            bytes_per_frame: bytes as f64 / 48.0,
            seconds_per_frame: encode_time / 48.0,
            similarity: scores.iter().sum::<f64>() / 48.0,
        });
    }
    if cancel.load(Ordering::Acquire) {
        return Err("작업이 취소되었습니다.".into());
    }
    if let Some(index) = choose(&results, settings.goal) {
        let selected = &results[index];
        let codec = match selected.choice.settings.codec {
            Codec::H264 => "H.264",
            Codec::H265 => "H.265",
            Codec::Av1 => "AV1",
        };
        let effort = match selected.choice.settings.effort {
            Effort::Speed => "속도 우선",
            Effort::Balanced => "균형",
            Effort::Efficiency => "효율 우선",
        };
        Ok(Recommendation{choice:selected.choice.clone(),note:format!("자동 추천 적용 · {codec} · {effort} · 비교 완료 {}개 후보 중 선택 (짧은 구간 화질 유사도 {:.3}, 전체 화질은 달라질 수 있어요)",results.len(),selected.similarity)})
    } else {
        Ok(Recommendation{choice:fallback,note:"자동 추천 · 제한 시간 내 화질 기준을 통과한 비교 결과가 없어 기본 설정을 유지합니다.".into()})
    }
}

fn fatal_trial_error(error: &ProcessError) -> bool {
    let text = error.to_string().to_lowercase();
    [
        "no space left",
        "disk full",
        "permission denied",
        "access is denied",
        "input/output error",
    ]
    .iter()
    .any(|message| text.contains(message))
}
