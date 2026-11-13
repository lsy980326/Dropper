use dropper_lib::{
    acceleration::{Backend, Capabilities, EncodingProfile, HardwareEncoder, WorkBudget},
    command::{encode_args_with_profile, Acceleration, Codec, Settings},
    media::Media,
};
use serde_json::json;
use std::path::Path;

fn media() -> Media {
    Media::from_probe(json!({"streams":[{"index":0,"codec_type":"video","codec_name":"h264","width":1280,"height":720,"pix_fmt":"yuv420p"}],"format":{"duration":"10800"}}), 2_540_000_000).unwrap()
}

#[test]
fn old_settings_remain_compatible_and_invalid_parallelism_is_rejected() {
    let settings: Settings =
        serde_json::from_value(json!({"quality":"balanced","codec":"h265","container":"auto"}))
            .unwrap();
    assert_eq!(settings.acceleration, Acceleration::Auto);
    assert_eq!(settings.parallelism, 0);
    let mut invalid = settings;
    invalid.parallelism = 255;
    assert!(WorkBudget::new(16, 10, &invalid).is_err());
}

#[test]
fn budgets_use_sixteen_logical_processors_without_spawning_unbounded_jobs() {
    let settings = Settings::default();
    let one = WorkBudget::new(16, 1, &settings).unwrap();
    assert_eq!((one.workers, one.threads_per_worker), (1, 16));
    let many = WorkBudget::new(16, 100, &settings).unwrap();
    assert_eq!((many.workers, many.threads_per_worker), (2, 8));
    let four = WorkBudget::new(
        16,
        100,
        &Settings {
            parallelism: 4,
            ..settings
        },
    )
    .unwrap();
    assert_eq!((four.workers, four.threads_per_worker), (4, 4));
    let tiny = WorkBudget::new(1, 100, &Settings::default()).unwrap();
    assert_eq!((tiny.workers, tiny.threads_per_worker), (1, 1));
}

#[test]
fn actual_capabilities_select_gpu_but_hdr_ten_bit_and_cpu_choice_stay_on_cpu() {
    let caps = Capabilities {
        codecs: vec![Codec::H265],
        hardware: vec![HardwareEncoder {
            codec: Codec::H265,
            backend: Backend::Nvidia,
        }],
        cpu_threads: 16,
    };
    assert_eq!(
        caps.profile(&media(), &Settings::default(), 8).backend,
        Backend::Nvidia
    );
    assert_eq!(
        caps.profile(
            &media(),
            &Settings {
                acceleration: Acceleration::Cpu,
                ..Settings::default()
            },
            16
        )
        .backend,
        Backend::Cpu
    );
    let mut hdr = media();
    hdr.hdr = Some("HDR10".into());
    assert_eq!(
        caps.profile(&hdr, &Settings::default(), 16).backend,
        Backend::Cpu
    );
    let mut ten_bit = media();
    ten_bit.video.pix_fmt = "yuv420p10le".into();
    assert_eq!(
        caps.profile(&ten_bit, &Settings::default(), 16).backend,
        Backend::Cpu
    );
}

#[test]
fn nvenc_uses_its_own_quality_controls_and_cpu_uses_the_assigned_budget() {
    let settings = Settings::default();
    let args = encode_args_with_profile(
        Path::new("in.mp4"),
        Path::new("out.mp4"),
        &media(),
        &settings,
        None,
        EncodingProfile {
            backend: Backend::Nvidia,
            threads: 8,
        },
    )
    .unwrap();
    assert!(args.windows(2).any(|p| p == ["-c:v:0", "hevc_nvenc"]));
    assert!(args.windows(2).any(|p| p == ["-rc:v:0", "vbr"]));
    assert!(args.windows(2).any(|p| p == ["-b:v:0", "0"]));
    assert!(!args.iter().any(
        |p| p.to_string_lossy().contains("crf") || p.to_string_lossy().contains("x265-params")
    ));
    let cpu = encode_args_with_profile(
        Path::new("in.mp4"),
        Path::new("out.mp4"),
        &media(),
        &settings,
        None,
        EncodingProfile::cpu(16),
    )
    .unwrap();
    assert!(cpu.windows(2).any(|p| p == ["-preset:v:0", "fast"]));
    assert!(cpu.iter().any(|p| p.to_string_lossy().contains("pools=16")));
}

#[test]
fn videotoolbox_never_silently_uses_a_software_encoder() {
    let args = encode_args_with_profile(
        Path::new("in.mp4"),
        Path::new("out.mp4"),
        &media(),
        &Settings::default(),
        None,
        EncodingProfile {
            backend: Backend::Apple,
            threads: 8,
        },
    )
    .unwrap();
    assert!(args.windows(2).any(|p| p == ["-allow_sw:v:0", "0"]));
}

#[test]
fn unsafe_gpu_pixel_formats_are_rejected_at_the_command_boundary() {
    let mut source = media();
    source.video.pix_fmt = "p016le".into();
    assert!(encode_args_with_profile(
        Path::new("in.mp4"),
        Path::new("out.mp4"),
        &source,
        &Settings::default(),
        None,
        EncodingProfile {
            backend: Backend::Nvidia,
            threads: 8
        }
    )
    .is_err());
}

#[test]
fn detected_hardware_really_encodes_and_keeps_originals() {
    use dropper_lib::engine::Tools;
    use std::{fs, process::Command, sync::atomic::AtomicBool};
    let tools = Tools::system().unwrap();
    let caps = Capabilities::detect(&tools).unwrap();
    println!("Detected hardware: {:?}", caps.hardware);
    let temp = tempfile::tempdir().unwrap();
    let input = temp.path().join("source.mkv");
    assert!(Command::new(&tools.ffmpeg)
        .args([
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=320x180:rate=24",
            "-f",
            "lavfi",
            "-i",
            "sine=sample_rate=48000",
            "-t",
            "2",
            "-vf",
            "select='if(lt(n,12),1,not(mod(n,3)))'",
            "-fps_mode",
            "passthrough",
            "-c:v",
            "ffv1",
            "-c:a",
            "pcm_s16le"
        ])
        .arg(&input)
        .status()
        .unwrap()
        .success());
    let original = fs::read(&input).unwrap();
    let cancel = AtomicBool::new(false);
    let media = tools.probe(&input, &cancel).unwrap();
    let timestamps = |path: &Path| -> Vec<f64> {
        let output = Command::new(&tools.ffprobe)
            .args([
                "-v",
                "error",
                "-select_streams",
                "v:0",
                "-show_frames",
                "-show_entries",
                "frame=best_effort_timestamp_time",
                "-of",
                "json",
            ])
            .arg(path)
            .output()
            .unwrap();
        assert!(output.status.success());
        let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        value["frames"]
            .as_array()
            .unwrap()
            .iter()
            .map(|frame| {
                frame["best_effort_timestamp_time"]
                    .as_str()
                    .unwrap()
                    .parse()
                    .unwrap()
            })
            .collect()
    };
    let before = timestamps(&input);
    assert!(before.windows(2).any(|p| p[1] - p[0] > 0.1));
    for hw in caps.hardware {
        let settings = Settings {
            codec: hw.codec,
            ..Settings::default()
        };
        let profile = EncodingProfile {
            backend: hw.backend,
            threads: caps.cpu_threads,
        };
        let out = temp.path().join(format!("{:?}", hw.codec));
        let estimate = tools
            .estimate_with_profile(&input, &media, &settings, &cancel, &mut |_| {}, profile)
            .unwrap();
        assert!(estimate > 0);
        let result = tools
            .compress_with_profile(
                &input,
                &media,
                &settings,
                &out,
                &cancel,
                &mut |_| {},
                profile,
            )
            .unwrap();
        assert!(result.saved);
        let output_path = result.output_path.unwrap();
        let encoded = tools.probe(Path::new(&output_path), &cancel).unwrap();
        assert_eq!(
            (encoded.video.width, encoded.video.height),
            (Some(320), Some(180))
        );
        assert!((encoded.duration - media.duration).abs() < 0.1);
        assert_eq!(
            encoded
                .streams
                .iter()
                .filter(|stream| stream.codec_type == "audio")
                .count(),
            1
        );
        let after = timestamps(Path::new(&output_path));
        assert_eq!(before.len(), after.len());
        assert!(before.iter().zip(after).all(|(a, b)| (a - b).abs() < 0.002));
        assert_eq!(fs::read(&input).unwrap(), original);
    }
}
