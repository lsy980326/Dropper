use dropper_lib::{
    command::{Codec, Settings},
    engine::Tools,
    jobs::{JobKind, Runtime},
};
use std::{
    fs,
    process::Command,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

fn ffmpeg(args: &[&str], output: &std::path::Path) {
    let result = Command::new(Tools::system().unwrap().ffmpeg)
        .args(["-hide_banner", "-loglevel", "error"])
        .args(args)
        .arg(output)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}

#[test]
fn unknown_matroska_audio_bitrate_still_gets_a_sample_based_estimate() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("audio.mkv");
    ffmpeg(
        &[
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=160x96:rate=12",
            "-f",
            "lavfi",
            "-i",
            "sine=sample_rate=48000",
            "-t",
            "2",
            "-c:v",
            "ffv1",
            "-c:a",
            "aac",
        ],
        &path,
    );
    let tools = Tools::system().unwrap();
    let cancel = AtomicBool::new(false);
    let media = tools.probe(&path, &cancel).unwrap();
    let estimate = tools
        .estimate(&path, &media, &Settings::default(), &cancel, &mut |_| {})
        .unwrap();
    assert!(estimate > 1000);
}

#[test]
fn h265_and_av1_produce_decodable_outputs() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("source.mkv");
    ffmpeg(
        &[
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=160x96:rate=12",
            "-t",
            "2",
            "-c:v",
            "ffv1",
        ],
        &path,
    );
    let tools = Tools::system().unwrap();
    let cancel = AtomicBool::new(false);
    let media = tools.probe(&path, &cancel).unwrap();
    for (codec, expected) in [(Codec::H265, "hevc"), (Codec::Av1, "av1")] {
        let settings = Settings {
            codec,
            ..Settings::default()
        };
        let out = tools
            .compress(
                &path,
                &media,
                &settings,
                &temp.path().join(expected),
                &cancel,
                &mut |_| {},
            )
            .unwrap();
        assert!(out.saved);
        let probe = tools
            .probe(std::path::Path::new(&out.output_path.unwrap()), &cancel)
            .unwrap();
        assert_eq!(probe.video.codec_name, expected);
        assert_eq!(probe.video.width, Some(160));
    }
}

#[test]
fn static_hdr_metadata_and_ten_bit_survive_real_h265_encode() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("hdr.mkv");
    ffmpeg(&["-f","lavfi","-i","testsrc2=size=160x96:rate=12,format=yuv420p10le","-t","2","-c:v","libx265","-color_primaries","bt2020","-color_trc","smpte2084","-colorspace","bt2020nc","-x265-params","lossless=1:pools=2:log-level=error:hdr10=1:colorprim=bt2020:transfer=smpte2084:colormatrix=bt2020nc:master-display=G(13250,34500)B(7500,3000)R(34000,16000)WP(15635,16450)L(10000000,1):max-cll=1000,400"],&path);
    let tools = Tools::system().unwrap();
    let cancel = AtomicBool::new(false);
    let media = tools.probe(&path, &cancel).unwrap();
    assert_eq!(media.hdr.as_deref(), Some("HDR10"));
    assert!(media
        .video
        .side_data_list
        .iter()
        .any(|s| s["side_data_type"] == "Mastering display metadata"));
    let out = tools
        .compress(
            &path,
            &media,
            &Settings::default(),
            &temp.path().join("out"),
            &cancel,
            &mut |_| {},
        )
        .unwrap();
    assert!(out.saved);
    let probe = tools
        .probe(std::path::Path::new(&out.output_path.unwrap()), &cancel)
        .unwrap();
    assert_eq!(probe.hdr.as_deref(), Some("HDR10"));
    assert_eq!(probe.video.pix_fmt, "yuv420p10le");
    for kind in ["Mastering display metadata", "Content light level metadata"] {
        let before = media
            .video
            .side_data_list
            .iter()
            .find(|s| s["side_data_type"] == kind)
            .unwrap();
        let after = probe
            .video
            .side_data_list
            .iter()
            .find(|s| s["side_data_type"] == kind)
            .unwrap();
        assert_eq!(before, after);
    }
}

#[test]
fn tiny_already_compressed_source_is_kept_without_a_larger_output() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("tiny.mp4");
    ffmpeg(
        &[
            "-f",
            "lavfi",
            "-i",
            "color=black:size=64x64:rate=12",
            "-t",
            "1",
            "-c:v",
            "libx264",
            "-crf",
            "50",
        ],
        &path,
    );
    let tools = Tools::system().unwrap();
    let cancel = AtomicBool::new(false);
    let media = tools.probe(&path, &cancel).unwrap();
    let dir = temp.path().join("out");
    let result = tools
        .compress(
            &path,
            &media,
            &Settings::default(),
            &dir,
            &cancel,
            &mut |_| {},
        )
        .unwrap();
    assert!(!result.saved);
    assert!(result.output_path.is_none());
    assert_eq!(fs::read_dir(dir).unwrap().count(), 0);
}

#[test]
fn manager_rejects_unregistered_and_changed_files_and_concurrent_work() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("source.mkv");
    ffmpeg(
        &[
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=160x96:rate=12",
            "-t",
            "1",
            "-c:v",
            "ffv1",
        ],
        &path,
    );
    let runtime = Runtime::new(Tools::system(), "system");
    let guard = runtime.begin().unwrap();
    assert!(runtime.begin().is_err());
    drop(guard);
    assert!(runtime
        .work(
            vec!["unregistered".into()],
            Settings::default(),
            None,
            JobKind::Compress,
            1,
            &mut |_| {}
        )
        .is_err());
    let files = runtime
        .add(vec![path.to_string_lossy().into_owned()])
        .unwrap();
    fs::write(&path, b"changed source").unwrap();
    let mut events = vec![];
    runtime
        .work(
            vec![files[0].id.clone()],
            Settings::default(),
            None,
            JobKind::Compress,
            2,
            &mut |e| events.push(e),
        )
        .unwrap();
    assert!(events
        .iter()
        .any(|e| e.phase == "error" && e.message.as_deref().unwrap_or("").contains("변경")));
    assert!(!temp.path().join("Compressed").exists());
}

#[test]
fn cancellation_terminates_an_already_running_ffmpeg() {
    let tools = Tools::system().unwrap();
    let cancel = AtomicBool::new(false);
    let start = Instant::now();
    let args = [
        "-v",
        "error",
        "-re",
        "-f",
        "lavfi",
        "-i",
        "testsrc2=size=160x96:rate=12",
        "-t",
        "10",
        "-progress",
        "pipe:1",
        "-f",
        "null",
        "-",
    ]
    .map(Into::into);
    let result = dropper_lib::process::execute(
        &tools.ffmpeg,
        &args,
        &cancel,
        Duration::from_secs(15),
        false,
        &mut |seconds| {
            if seconds > 0.0 {
                cancel.store(true, Ordering::Release);
            }
        },
    );
    assert!(result.unwrap_err().contains("취소"));
    assert!(start.elapsed() < Duration::from_secs(5));
}

#[test]
fn readding_a_changed_source_refreshes_its_analysis_instead_of_returning_stale_cache() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("source.mkv");
    ffmpeg(
        &[
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=160x96:rate=12",
            "-t",
            "1",
            "-c:v",
            "ffv1",
        ],
        &path,
    );
    let runtime = Runtime::new(Tools::system(), "system");
    let first = runtime
        .add(vec![path.to_string_lossy().into_owned()])
        .unwrap();
    fs::remove_file(&path).unwrap();
    ffmpeg(
        &[
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=320x180:rate=12",
            "-t",
            "1",
            "-c:v",
            "ffv1",
        ],
        &path,
    );
    let second = runtime
        .add(vec![path.to_string_lossy().into_owned()])
        .unwrap();
    assert_eq!(first[0].id, second[0].id);
    assert_eq!(second[0].width, 320);
}

#[test]
fn real_multiple_audio_and_subtitles_are_retained_in_matroska() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("tracks.mkv");
    let subtitle = temp.path().join("sub.srt");
    fs::write(&subtitle, "1\n00:00:00,000 --> 00:00:01,000\n드롭퍼 검증\n").unwrap();
    let sub = subtitle.to_string_lossy();
    ffmpeg(
        &[
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=160x96:rate=12",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=880",
            "-i",
            &sub,
            "-t",
            "2",
            "-map",
            "0:v",
            "-map",
            "1:a",
            "-map",
            "2:a",
            "-map",
            "3:s",
            "-c:v",
            "ffv1",
            "-c:a",
            "pcm_s16le",
            "-c:s",
            "srt",
            "-metadata:s:a:0",
            "language=kor",
            "-metadata:s:a:1",
            "language=eng",
        ],
        &path,
    );
    let tools = Tools::system().unwrap();
    let cancel = AtomicBool::new(false);
    let media = tools.probe(&path, &cancel).unwrap();
    let result = tools
        .compress(
            &path,
            &media,
            &Settings::default(),
            &temp.path().join("out"),
            &cancel,
            &mut |_| {},
        )
        .unwrap();
    let output_path = result.output_path.unwrap();
    assert!(output_path.ends_with(".mkv"));
    let output = tools
        .probe(std::path::Path::new(&output_path), &cancel)
        .unwrap();
    let audio: Vec<_> = output
        .streams
        .iter()
        .filter(|s| s.codec_type == "audio")
        .collect();
    assert_eq!(audio.len(), 2);
    assert_eq!(audio[0].tags["language"], "kor");
    assert_eq!(audio[1].tags["language"], "eng");
    assert_eq!(
        output
            .streams
            .iter()
            .filter(|s| s.codec_type == "subtitle")
            .count(),
        1
    );
}

#[test]
fn rotated_phone_video_keeps_its_visible_orientation() {
    let temp = tempfile::tempdir().unwrap();
    let raw = temp.path().join("raw.mov");
    let path = temp.path().join("rotated.mov");
    ffmpeg(
        &[
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=160x96:rate=12",
            "-t",
            "2",
            "-c:v",
            "prores_ks",
        ],
        &raw,
    );
    ffmpeg(
        &[
            "-display_rotation:v:0",
            "90",
            "-i",
            &raw.to_string_lossy(),
            "-c",
            "copy",
        ],
        &path,
    );
    let tools = Tools::system().unwrap();
    let cancel = AtomicBool::new(false);
    let media = tools.probe(&path, &cancel).unwrap();
    let result = tools
        .compress(
            &path,
            &media,
            &Settings::default(),
            &temp.path().join("out"),
            &cancel,
            &mut |_| {},
        )
        .unwrap();
    let output = tools
        .probe(std::path::Path::new(&result.output_path.unwrap()), &cancel)
        .unwrap();
    assert!(media
        .video
        .side_data_list
        .iter()
        .any(|s| s["rotation"].as_i64().map(|r| r.abs()) == Some(90)));
    assert_eq!(
        (output.video.width, output.video.height),
        (Some(96), Some(160))
    );
}

#[test]
fn variable_frame_timestamps_survive_without_frame_duplication() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("vfr.mkv");
    ffmpeg(
        &[
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=160x96:rate=12",
            "-t",
            "3",
            "-vf",
            "select=if(lt(n\\,12)\\,not(mod(n\\,2))\\,not(mod(n\\,3)))",
            "-fps_mode",
            "vfr",
            "-c:v",
            "ffv1",
        ],
        &path,
    );
    let tools = Tools::system().unwrap();
    let cancel = AtomicBool::new(false);
    let media = tools.probe(&path, &cancel).unwrap();
    let result = tools
        .compress(
            &path,
            &media,
            &Settings::default(),
            &temp.path().join("out"),
            &cancel,
            &mut |_| {},
        )
        .unwrap();
    let frames = |path: &std::path::Path| {
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
        let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        value["frames"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| {
                v["best_effort_timestamp_time"]
                    .as_str()
                    .unwrap()
                    .parse::<f64>()
                    .unwrap()
            })
            .collect::<Vec<_>>()
    };
    let before = frames(&path);
    let after = frames(std::path::Path::new(&result.output_path.unwrap()));
    assert_eq!(before.len(), after.len());
    assert!(before
        .iter()
        .zip(after.iter())
        .all(|(a, b)| (a - b).abs() < 0.002));
}
