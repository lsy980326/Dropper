#![cfg(target_os = "macos")]

use dropper_lib::{
    acceleration::cpu_threads,
    command::Settings,
    engine::Tools,
    jobs::{JobKind, Runtime},
};
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::AtomicBool,
};

#[test]
#[ignore = "Requires a DMG-installed app; run through scripts/verify-macos-installer.py"]
fn installed_engine_recommends_and_compresses_a_folder_without_changing_originals() {
    let app = PathBuf::from(
        std::env::var_os("DROPPER_INSTALLED_APP").expect("Installed app path required"),
    );
    let bin = app.join("Contents/Resources/media/bin");
    let tools = Tools {
        ffmpeg: bin.join("ffmpeg"),
        ffprobe: bin.join("ffprobe"),
    };
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("내 영상");
    fs::create_dir_all(root.join("하위 폴더")).unwrap();
    let first = root.join("원본 영상.mkv");
    let second = root.join("하위 폴더/다른 영상.mkv");
    let output = Command::new(&tools.ffmpeg)
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
            "sine=frequency=440:sample_rate=48000",
            "-t",
            "31",
            "-c:v",
            "ffv1",
            "-c:a",
            "pcm_s16le",
        ])
        .arg(&first)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    fs::copy(&first, &second).unwrap();
    let original = fs::read(&first).unwrap();
    let cancel = AtomicBool::new(false);
    let source = tools.probe(&first, &cancel).unwrap();
    let runtime = Runtime::new(Ok(tools.clone()), "bundled");
    let environment = runtime.environment();
    assert!(environment.ready, "{:?}", environment.error);
    assert_eq!(environment.source, "bundled");
    println!(
        "Installed engine hardware candidates: {:?}",
        environment.gpu_encoders
    );
    let imported = runtime
        .import_paths(
            vec![root.to_string_lossy().into_owned()],
            None,
            1,
            &mut |_| {},
        )
        .unwrap();
    assert_eq!(imported.files.len(), 2);
    let mut events = Vec::new();
    runtime
        .work(
            imported.files.iter().map(|item| item.id.clone()).collect(),
            Settings {
                recommendation: true,
                parallelism: 2,
                ..Settings::default()
            },
            None,
            JobKind::Compress,
            2,
            &mut |event| events.push(event),
        )
        .unwrap();
    let mut active = HashSet::new();
    let mut peak = 0;
    for event in &events {
        if event.phase == "processing" {
            active.insert(&event.id);
            peak = peak.max(active.len());
        } else {
            active.remove(&event.id);
        }
    }
    assert_eq!(peak, cpu_threads().min(2));
    let done: Vec<_> = events
        .iter()
        .filter(|event| event.phase == "done")
        .collect();
    assert_eq!(
        done.len(),
        2,
        "{:?}",
        events
            .iter()
            .filter_map(|e| e.message.as_deref())
            .collect::<Vec<_>>()
    );
    for event in done {
        assert!(event.backend.is_some());
        assert!(
            event
                .backend_note
                .as_deref()
                .unwrap_or("")
                .contains("자동 추천 적용"),
            "{:?}",
            event.backend_note
        );
        let path = Path::new(event.output_path.as_deref().unwrap());
        assert!(path.starts_with(root.join("Compressed")));
        let actual = tools.probe(path, &cancel).unwrap();
        assert_eq!(actual.video.width, source.video.width);
        assert_eq!(actual.video.height, source.video.height);
        assert!((actual.duration - source.duration).abs() < 0.15);
        assert_eq!(
            actual
                .streams
                .iter()
                .filter(|s| s.codec_type == "audio")
                .count(),
            1
        );
        assert!(actual.bytes < source.bytes);
    }
    assert_eq!(fs::read(first).unwrap(), original);
    assert_eq!(fs::read(second).unwrap(), original);
    println!("Installed bundle: automatic recommendations, parallel folder batch, tracks and originals verified.");
}
