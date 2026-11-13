use dropper_lib::{
    acceleration::{Backend, Capabilities, EncodingProfile, HardwareEncoder},
    command::{encode_args_with_profile, Acceleration, Codec, Effort, Goal, Settings},
    media::Media,
    recommendation::{candidates, choose, Measurement},
};
use serde_json::json;
use std::path::Path;

fn media() -> Media {
    Media::from_probe(json!({"streams":[{"index":0,"codec_type":"video","codec_name":"h264","width":1280,"height":720,"pix_fmt":"yuv420p"}],"format":{"duration":"120"}}),20_000_000).unwrap()
}
fn capabilities() -> Capabilities {
    Capabilities {
        codecs: vec![Codec::H264, Codec::H265, Codec::Av1],
        hardware: vec![
            HardwareEncoder {
                codec: Codec::H265,
                backend: Backend::Nvidia,
            },
            HardwareEncoder {
                codec: Codec::H265,
                backend: Backend::Intel,
            },
            HardwareEncoder {
                codec: Codec::Av1,
                backend: Backend::Amd,
            },
        ],
        cpu_threads: 16,
    }
}

#[test]
fn all_available_backends_participate_and_explicit_device_choices_are_respected() {
    let caps = capabilities();
    let settings = Settings {
        recommendation: true,
        ..Settings::default()
    };
    let choices = candidates(&caps, &media(), &settings, 8);
    for backend in [Backend::Cpu, Backend::Nvidia, Backend::Intel, Backend::Amd] {
        assert!(choices
            .iter()
            .any(|choice| choice.profile.backend == backend));
    }
    let cpu = candidates(
        &caps,
        &media(),
        &Settings {
            acceleration: Acceleration::Cpu,
            ..settings.clone()
        },
        8,
    );
    assert!(cpu
        .iter()
        .all(|choice| choice.profile.backend == Backend::Cpu));
    let amd = candidates(
        &caps,
        &media(),
        &Settings {
            acceleration: Acceleration::Amd,
            ..settings
        },
        8,
    );
    assert!(amd
        .iter()
        .all(|choice| choice.profile.backend == Backend::Amd));
}

#[test]
fn hdr_recommendations_preserve_verified_ten_bit_cpu_hevc_path() {
    let mut source = media();
    source.hdr = Some("HDR10".into());
    source.video.pix_fmt = "yuv420p10le".into();
    let choices = candidates(&capabilities(), &source, &Settings::default(), 8);
    assert!(!choices.is_empty());
    assert!(choices.iter().all(
        |choice| choice.settings.codec == Codec::H265 && choice.profile.backend == Backend::Cpu
    ));
}

#[test]
fn fastest_smallest_and_quality_goals_use_measured_results_not_vendor_order() {
    let choices = candidates(&capabilities(), &media(), &Settings::default(), 8);
    let results = vec![
        Measurement {
            choice: choices[0].clone(),
            bytes_per_frame: 100.0,
            seconds_per_frame: 0.1,
            similarity: 0.97,
        },
        Measurement {
            choice: choices[1].clone(),
            bytes_per_frame: 60.0,
            seconds_per_frame: 0.3,
            similarity: 0.97,
        },
        Measurement {
            choice: choices[2].clone(),
            bytes_per_frame: 90.0,
            seconds_per_frame: 0.2,
            similarity: 0.99,
        },
    ];
    assert_eq!(choose(&results, Goal::Speed).unwrap(), 0);
    assert_eq!(choose(&results, Goal::Size).unwrap(), 1);
    assert_eq!(choose(&results, Goal::Quality).unwrap(), 2);
    assert!(choose(
        &[Measurement {
            similarity: f64::NAN,
            ..results[0].clone()
        }],
        Goal::Balanced
    )
    .is_none());
}

#[test]
fn extremes_use_each_encoders_actual_effort_options() {
    for (backend, codec, flag, fast, slow) in [
        (
            Backend::Cpu,
            Codec::H265,
            "-preset:v:0",
            "ultrafast",
            "veryslow",
        ),
        (Backend::Nvidia, Codec::H265, "-preset:v:0", "p1", "p7"),
        (
            Backend::Intel,
            Codec::Av1,
            "-preset:v:0",
            "veryfast",
            "veryslow",
        ),
        (Backend::Amd, Codec::Av1, "-quality:v:0", "speed", "quality"),
        (Backend::Apple, Codec::H265, "-prio_speed:v:0", "1", "0"),
    ] {
        for (effort, value) in [(Effort::Speed, fast), (Effort::Efficiency, slow)] {
            let args = encode_args_with_profile(
                Path::new("in.mkv"),
                Path::new("out.mkv"),
                &media(),
                &Settings {
                    codec,
                    effort,
                    ..Settings::default()
                },
                None,
                EncodingProfile {
                    backend,
                    threads: 8,
                },
            )
            .unwrap();
            assert!(args.windows(2).any(|p| p == [flag, value]), "{backend:?}");
            if backend == Backend::Intel && codec == Codec::Av1 {
                assert!(!args.iter().any(|arg| arg == "-look_ahead:v:0"));
            }
        }
    }
}

#[test]
fn real_sample_comparison_applies_and_preserves_original() {
    use dropper_lib::{engine::Tools, recommendation::recommend};
    use std::{process::Command, sync::atomic::AtomicBool};
    let tools = Tools::system().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("추천 시험.mkv");
    assert!(Command::new(&tools.ffmpeg)
        .args([
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=320x180:rate=24",
            "-t",
            "31",
            "-c:v",
            "ffv1"
        ])
        .arg(&path)
        .status()
        .unwrap()
        .success());
    let original = std::fs::read(&path).unwrap();
    let cancel = AtomicBool::new(false);
    let media = tools.probe(&path, &cancel).unwrap();
    let caps = Capabilities {
        codecs: vec![Codec::H264],
        hardware: vec![],
        cpu_threads: 2,
    };
    let result = recommend(
        &tools,
        &path,
        &media,
        &Settings {
            codec: Codec::H264,
            recommendation: true,
            ..Settings::default()
        },
        &caps,
        2,
        &cancel,
    )
    .unwrap();
    assert!(result.note.contains("자동 추천 적용"), "{}", result.note);
    assert_eq!(result.choice.settings.codec, Codec::H264);
    assert_eq!(std::fs::read(&path).unwrap(), original);
    cancel.store(true, std::sync::atomic::Ordering::Release);
    assert!(recommend(
        &tools,
        &path,
        &media,
        &Settings::default(),
        &caps,
        2,
        &cancel
    )
    .is_err());
}

#[test]
fn short_hdr_and_unavailable_default_codec_use_a_valid_fallback() {
    use dropper_lib::{engine::Tools, recommendation::recommend};
    use std::sync::atomic::AtomicBool;
    let tools = Tools::system().unwrap();
    let mut source = media();
    source.duration = 5.0;
    let cancel = AtomicBool::new(false);
    let cpu = Capabilities {
        codecs: vec![Codec::H264],
        hardware: vec![],
        cpu_threads: 2,
    };
    let result = recommend(
        &tools,
        Path::new("unused"),
        &source,
        &Settings::default(),
        &cpu,
        2,
        &cancel,
    )
    .unwrap();
    assert_eq!(result.choice.settings.codec, Codec::H264);
    source.hdr = Some("HDR10".into());
    source.video.pix_fmt = "yuv420p10le".into();
    let result = recommend(
        &tools,
        Path::new("unused"),
        &source,
        &Settings {
            codec: Codec::Av1,
            ..Settings::default()
        },
        &capabilities(),
        2,
        &cancel,
    )
    .unwrap();
    assert_eq!(result.choice.settings.codec, Codec::H265);
    assert_eq!(result.choice.profile.backend, Backend::Cpu);
    assert!(recommend(
        &tools,
        Path::new("unused"),
        &source,
        &Settings::default(),
        &cpu,
        2,
        &cancel
    )
    .is_err());
    cancel.store(true, std::sync::atomic::Ordering::Release);
    assert!(recommend(
        &tools,
        Path::new("unused"),
        &source,
        &Settings::default(),
        &capabilities(),
        2,
        &cancel
    )
    .is_err());
}

#[test]
fn real_extreme_efforts_encode_on_local_software_and_hardware() {
    use dropper_lib::{command::encode_args_with_profile, engine::Tools};
    use std::{process::Command, sync::atomic::AtomicBool};
    let tools = Tools::system().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("effort.mkv");
    assert!(Command::new(&tools.ffmpeg)
        .args([
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=320x180:rate=24",
            "-t",
            "1",
            "-c:v",
            "ffv1"
        ])
        .arg(&input)
        .status()
        .unwrap()
        .success());
    let cancel = AtomicBool::new(false);
    let source = tools.probe(&input, &cancel).unwrap();
    let caps = Capabilities::detect(&tools).unwrap();
    let mut profiles: Vec<_> = caps.codecs.iter().map(|c| (*c, Backend::Cpu)).collect();
    profiles.extend(caps.hardware.iter().map(|h| (h.codec, h.backend)));
    for (codec, backend) in profiles {
        for effort in [Effort::Speed, Effort::Efficiency] {
            let output = dir
                .path()
                .join(format!("{codec:?}-{backend:?}-{effort:?}.mkv"));
            let args = encode_args_with_profile(
                &input,
                &output,
                &source,
                &Settings {
                    codec,
                    effort,
                    ..Settings::default()
                },
                Some((0.0, 0.5)),
                EncodingProfile {
                    backend,
                    threads: 2,
                },
            )
            .unwrap();
            let result = Command::new(&tools.ffmpeg).args(args).output().unwrap();
            assert!(
                result.status.success(),
                "{codec:?} {backend:?} {effort:?}: {}",
                String::from_utf8_lossy(&result.stderr)
            );
            let actual = tools.probe(&output, &cancel).unwrap();
            assert_eq!(actual.video.width, source.video.width);
            assert!((actual.duration - 0.5).abs() < 0.1);
        }
    }
}
