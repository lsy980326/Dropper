use dropper_lib::{
    command::{Acceleration, Codec, Container, Settings},
    engine::Tools,
    folder,
    jobs::{JobKind, Runtime},
};
use std::{fs, process::Command, sync::atomic::AtomicBool};
#[test]
fn mixed_folder_reports_every_file_and_moves_only_verified_successes() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    fs::create_dir(root.join("Compressed")).unwrap();
    let tools = Tools::system().unwrap();
    let mp4 = root.join("camera.mp4");
    let status = Command::new(&tools.ffmpeg)
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
            "-c:v",
            "libx264",
            "-qp",
            "0",
            "-preset",
            "ultrafast",
            "-c:a",
            "aac",
            "-timecode",
            "00:00:00:00",
        ])
        .arg(&mp4)
        .status()
        .unwrap();
    assert!(status.success());
    let nested = root.join("Compressed/original.mp4");
    fs::copy(&mp4, &nested).unwrap();
    let avi = root.join("audio.avi");
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
            "-c:v",
            "ffv1",
            "-c:a",
            "pcm_s16le"
        ])
        .arg(&avi)
        .status()
        .unwrap()
        .success());
    let broken = root.join("broken.mp4");
    fs::write(&broken, b"broken input").unwrap();
    let original = fs::read(&mp4).unwrap();
    let avi_bytes = fs::read(&avi).unwrap();
    let probe = tools.probe(&mp4, &AtomicBool::new(false)).unwrap();
    assert_eq!(probe.auxiliary_tracks().len(), 1);
    let runtime = Runtime::new(Ok(tools.clone()), "test");
    let imported = runtime
        .import_paths(
            vec![root.to_string_lossy().into_owned()],
            None,
            1,
            &mut |_| {},
        )
        .unwrap();
    assert_eq!(imported.files.len(), 4);
    assert_eq!(
        imported
            .files
            .iter()
            .filter(|f| f.blocked.is_some())
            .count(),
        1
    );
    let ids = imported
        .files
        .iter()
        .map(|f| f.id.clone())
        .collect::<Vec<_>>();
    let settings = Settings {
        archive_originals: true,
        strip_data: true,
        codec: Codec::H264,
        container: Container::Mp4,
        acceleration: Acceleration::Cpu,
        parallelism: 2,
        ..Settings::default()
    };
    let mut events = Vec::new();
    runtime
        .work(ids, settings, None, JobKind::Compress, 2, &mut |e| {
            events.push(e)
        })
        .unwrap();
    let done: Vec<_> = events.iter().filter(|e| e.phase == "done").collect();
    assert_eq!(
        done.len(),
        2,
        "{:?}",
        events
            .iter()
            .filter_map(|e| e.message.as_deref())
            .collect::<Vec<_>>()
    );
    assert_eq!(events.iter().filter(|e| e.phase == "error").count(), 2);
    for event in done {
        assert!(event.original_moved, "{:?}", event.archive_warning);
        assert_eq!(
            fs::read(event.archived_path.as_deref().unwrap()).unwrap(),
            original
        );
        let actual = tools
            .probe(
                std::path::Path::new(event.output_path.as_deref().unwrap()),
                &AtomicBool::new(false),
            )
            .unwrap();
        assert!(actual.auxiliary_tracks().is_empty());
        assert_eq!(
            actual
                .streams
                .iter()
                .filter(|s| s.codec_type == "audio")
                .count(),
            1
        );
        assert!(event.report_path.is_some());
    }
    assert!(!mp4.exists());
    assert!(!nested.exists());
    assert_eq!(fs::read(avi).unwrap(), avi_bytes);
    assert_eq!(fs::read(broken).unwrap(), b"broken input");
    let report = fs::read_to_string(root.join("Compressed/Dropper-results.csv")).unwrap();
    for name in ["camera.mp4", "original.mp4", "audio.avi", "broken.mp4"] {
        assert!(report.contains(name), "{report}");
    }
    let remaining = folder::scan(&root, None, &AtomicBool::new(false), &mut |_| {}).unwrap();
    assert_eq!(remaining.files.len(), 2);
}

#[test]
fn reencoding_an_archived_single_file_reuses_the_initial_output_base() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    let input = root.join("original.mkv");
    let tools = Tools::system().unwrap();
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
    let original = fs::read(&input).unwrap();
    let runtime = Runtime::new(Ok(tools), "test");
    let ids = runtime
        .add(vec![input.to_string_lossy().into_owned()])
        .unwrap()
        .into_iter()
        .map(|f| f.id)
        .collect::<Vec<_>>();
    let settings = Settings {
        archive_originals: true,
        strip_data: true,
        codec: Codec::H264,
        acceleration: Acceleration::Cpu,
        ..Settings::default()
    };
    for run in [1, 2] {
        let mut events = Vec::new();
        runtime
            .work(
                ids.clone(),
                settings.clone(),
                None,
                JobKind::Compress,
                run,
                &mut |e| events.push(e),
            )
            .unwrap();
        let end = events.last().unwrap();
        assert_eq!(end.phase, "done", "{:?}", end.message);
        assert!(std::path::Path::new(end.output_path.as_deref().unwrap())
            .starts_with(root.join("Compressed/Results")));
        assert!(end.original_moved);
        assert_eq!(
            fs::read(end.archived_path.as_deref().unwrap()).unwrap(),
            original
        );
    }
    assert!(!root.join("Compressed/Originals/Compressed").exists());
}

#[test]
fn blocked_media_gets_a_terminal_event_and_record_without_encoding_or_moving() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let tools = Tools::system().unwrap();
    let input = root.join("two-tracks.mkv");
    assert!(Command::new(&tools.ffmpeg)
        .args([
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=160x90:rate=10",
            "-t",
            "0.5",
            "-map",
            "0:v",
            "-map",
            "0:v",
            "-c:v",
            "ffv1",
        ])
        .arg(&input)
        .status()
        .unwrap()
        .success());
    let original = fs::read(&input).unwrap();
    let runtime = Runtime::new(Ok(tools), "test");
    let files = runtime
        .add(vec![input.to_string_lossy().into_owned()])
        .unwrap();
    assert!(files[0].blocked.is_some());
    let mut events = Vec::new();
    runtime
        .work(
            vec![files[0].id.clone()],
            Settings {
                archive_originals: true,
                strip_data: true,
                ..Settings::default()
            },
            None,
            JobKind::Compress,
            1,
            &mut |e| events.push(e),
        )
        .unwrap();
    let end = events.last().unwrap();
    assert_eq!(end.phase, "error");
    assert!(!end.original_moved);
    assert!(fs::read_to_string(end.report_path.as_deref().unwrap())
        .unwrap()
        .contains("two-tracks.mkv"));
    assert_eq!(fs::read(input).unwrap(), original);
    assert!(!root.join("Compressed/Results").exists());
}

#[test]
fn no_gain_and_cancelled_jobs_keep_originals_and_log_the_outcome() {
    use dropper_lib::command::Quality;
    use std::sync::atomic::Ordering;
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let tools = Tools::system().unwrap();
    let input = root.join("small.mp4");
    assert!(Command::new(&tools.ffmpeg)
        .args([
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=320x180:rate=24",
            "-t",
            "2",
            "-c:v",
            "libx264",
            "-crf",
            "51",
        ])
        .arg(&input)
        .status()
        .unwrap()
        .success());
    let original = fs::read(&input).unwrap();
    let runtime = Runtime::new(Ok(tools), "test");
    let files = runtime
        .add(vec![input.to_string_lossy().into_owned()])
        .unwrap();
    let settings = Settings {
        archive_originals: true,
        strip_data: true,
        codec: Codec::H264,
        quality: Quality::High,
        acceleration: Acceleration::Cpu,
        ..Settings::default()
    };
    let mut events = Vec::new();
    runtime
        .work(
            vec![files[0].id.clone()],
            settings.clone(),
            None,
            JobKind::Compress,
            1,
            &mut |e| events.push(e),
        )
        .unwrap();
    assert_eq!(events.last().unwrap().phase, "unchanged");
    assert_eq!(fs::read(&input).unwrap(), original);
    events.clear();
    runtime
        .work(
            vec![files[0].id.clone()],
            settings,
            None,
            JobKind::Compress,
            2,
            &mut |e| {
                if e.phase == "processing" {
                    runtime.cancel.store(true, Ordering::Release);
                }
                events.push(e);
            },
        )
        .unwrap();
    assert_eq!(events.last().unwrap().phase, "cancelled");
    assert_eq!(fs::read(&input).unwrap(), original);
    assert!(!root.join("Compressed/Originals").exists());
    let report = fs::read_to_string(root.join("Compressed/Dropper-results.csv")).unwrap();
    assert!(report.contains("압축 이득 없음") && report.contains("취소"));
}

#[test]
fn organized_results_still_skip_reimport_and_write_records_with_archiving_off() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let tools = Tools::system().unwrap();
    let input = root.join("keep.mp4");
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
            "libx264",
            "-qp",
            "0",
            "-preset",
            "ultrafast",
        ])
        .arg(&input)
        .status()
        .unwrap()
        .success());
    let original = fs::read(&input).unwrap();
    let runtime = Runtime::new(Ok(tools), "test");
    let files = runtime
        .import_paths(
            vec![root.to_string_lossy().into_owned()],
            None,
            1,
            &mut |_| {},
        )
        .unwrap();
    let mut events = Vec::new();
    runtime
        .work(
            files.files.into_iter().map(|f| f.id).collect(),
            Settings {
                archive_originals: false,
                organize_output: true,
                strip_data: true,
                codec: Codec::H264,
                acceleration: Acceleration::Cpu,
                ..Settings::default()
            },
            None,
            JobKind::Compress,
            1,
            &mut |e| events.push(e),
        )
        .unwrap();
    let end = events.last().unwrap();
    assert_eq!(end.phase, "done", "{:?}", end.message);
    assert!(!end.original_moved);
    assert!(std::path::Path::new(end.output_path.as_deref().unwrap())
        .starts_with(root.join("Compressed/Results")));
    assert!(std::path::Path::new(end.report_path.as_deref().unwrap()).is_file());
    assert_eq!(fs::read(&input).unwrap(), original);
    assert!(!root.join("Compressed/Originals").exists());
    let scan = folder::scan(&root, None, &AtomicBool::new(false), &mut |_| {}).unwrap();
    assert_eq!(scan.files, vec![input]);
}
