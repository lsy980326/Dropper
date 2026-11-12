use dropper_lib::{
    command::{Codec, Settings},
    engine::Tools,
};
use std::{fs, process::Command, sync::atomic::AtomicBool};

fn source(dir: &std::path::Path) -> std::path::PathBuf {
    let path = dir.join("영상 $(특수문자).mkv");
    let status = Command::new(Tools::system().unwrap().ffmpeg)
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=320x180:rate=24",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:sample_rate=48000",
            "-t",
            "2",
            "-c:v",
            "ffv1",
            "-c:a",
            "pcm_s16le",
        ])
        .arg(&path)
        .status()
        .unwrap();
    assert!(status.success());
    path
}

#[test]
fn real_estimate_and_compression_preserve_source_and_never_overwrite() {
    let temp = tempfile::tempdir().unwrap();
    let path = source(temp.path());
    let original = fs::read(&path).unwrap();
    let tools = Tools::system().unwrap();
    let cancel = AtomicBool::new(false);
    let media = tools.probe(&path, &cancel).unwrap();
    assert_eq!(media.video.width, Some(320));
    let settings = Settings {
        codec: Codec::H264,
        ..Settings::default()
    };
    let estimate = tools
        .estimate(&path, &media, &settings, &cancel, &mut |_| {})
        .unwrap();
    assert!(estimate > 0 && estimate < media.bytes);
    let out = temp.path().join("Compressed");
    fs::create_dir(&out).unwrap();
    let existing = out.join("영상 $(특수문자)_compressed.mkv");
    fs::write(&existing, b"USER FILE").unwrap();
    let result = tools
        .compress(&path, &media, &settings, &out, &cancel, &mut |_| {})
        .unwrap();
    assert!(result.saved);
    let output_path = result.output_path.unwrap();
    assert_ne!(std::path::Path::new(&output_path), existing);
    assert_eq!(fs::read(&existing).unwrap(), b"USER FILE");
    assert_eq!(fs::read(&path).unwrap(), original);
    let output = tools
        .probe(std::path::Path::new(&output_path), &cancel)
        .unwrap();
    assert_eq!(
        output
            .streams
            .iter()
            .filter(|s| s.codec_type == "audio")
            .count(),
        1
    );
    assert!((output.duration - 2.0).abs() < 0.15);
    assert!(output.bytes < media.bytes);
    assert_eq!(fs::read_dir(out).unwrap().count(), 2);
}

#[test]
fn cancellation_leaves_no_partial_output_and_no_source_changes() {
    let temp = tempfile::tempdir().unwrap();
    let path = source(temp.path());
    let original = fs::read(&path).unwrap();
    let tools = Tools::system().unwrap();
    let media = tools.probe(&path, &AtomicBool::new(false)).unwrap();
    let out = temp.path().join("Compressed");
    let error = tools
        .compress(
            &path,
            &media,
            &Settings::default(),
            &out,
            &AtomicBool::new(true),
            &mut |_| {},
        )
        .unwrap_err();
    assert!(error.contains("취소"));
    assert_eq!(fs::read(path).unwrap(), original);
    assert!(!out.exists() || fs::read_dir(out).unwrap().count() == 0);
}

#[cfg(unix)]
#[test]
fn output_directory_symlink_is_rejected() {
    let temp = tempfile::tempdir().unwrap();
    let path = source(temp.path());
    let tools = Tools::system().unwrap();
    let cancel = AtomicBool::new(false);
    let media = tools.probe(&path, &cancel).unwrap();
    let actual = temp.path().join("actual");
    fs::create_dir(&actual).unwrap();
    let link = temp.path().join("Compressed");
    std::os::unix::fs::symlink(&actual, &link).unwrap();
    assert!(tools
        .compress(
            &path,
            &media,
            &Settings::default(),
            &link,
            &cancel,
            &mut |_| {}
        )
        .is_err());
    assert_eq!(fs::read_dir(actual).unwrap().count(), 0);
}
