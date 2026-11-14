use dropper_lib::{
    archive::{move_original, ArchivePlan, SourceSnapshot},
    folder::FolderSource,
};
use std::{fs, sync::atomic::AtomicBool};
#[test]
fn verified_success_moves_original_without_overwriting_and_keeps_relative_structure() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    let input = root.join("sub/clip.mp4");
    fs::create_dir(root.join("sub")).unwrap();
    fs::write(&input, b"original content").unwrap();
    let source = FolderSource {
        root: root.clone(),
        relative_path: "sub/clip.mp4".into(),
    };
    let plan = ArchivePlan::new(&input, Some(&source), None).unwrap();
    let mut snapshot = SourceSnapshot::capture(&input).unwrap();
    let result = plan.results_dir().unwrap().join("clip_compressed.mp4");
    fs::write(&result, b"compressed").unwrap();
    let backup = plan.originals_dir().unwrap().join("clip.mp4");
    fs::write(&backup, b"USER FILE").unwrap();
    let outcome = move_original(&mut snapshot, &plan, &result, &AtomicBool::new(false));
    assert!(outcome.moved, "{:?}", outcome.warning);
    assert!(!input.exists());
    assert_eq!(fs::read(backup).unwrap(), b"USER FILE");
    assert_eq!(
        fs::read(outcome.path.unwrap()).unwrap(),
        b"original content"
    );
    assert!(result.starts_with(root.join("Compressed/Results/sub")));
    assert!(outcome.receipt.unwrap().is_file());
}
#[test]
fn cancellation_or_replaced_source_never_removes_original() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    let input = root.join("clip.mp4");
    fs::write(&input, b"original").unwrap();
    let plan = ArchivePlan::new(&input, None, None).unwrap();
    let mut snapshot = SourceSnapshot::capture(&input).unwrap();
    let result = plan.results_dir().unwrap().join("clip_compressed.mp4");
    fs::write(&result, b"small").unwrap();
    let cancelled = move_original(&mut snapshot, &plan, &result, &AtomicBool::new(true));
    assert!(!cancelled.moved);
    assert_eq!(fs::read(&input).unwrap(), b"original");
    fs::rename(&input, root.join("old.mp4")).unwrap();
    fs::write(&input, b"replaced").unwrap();
    let outcome = move_original(&mut snapshot, &plan, &result, &AtomicBool::new(false));
    assert!(!outcome.moved);
    assert_eq!(fs::read(input).unwrap(), b"replaced");
}
#[cfg(unix)]
#[test]
fn backup_symlink_is_rejected_without_changing_source() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    let input = root.join("clip.mp4");
    fs::write(&input, b"original").unwrap();
    let plan = ArchivePlan::new(&input, None, None).unwrap();
    let result = plan.results_dir().unwrap().join("out.mp4");
    fs::write(&result, b"small").unwrap();
    let outside = root.join("outside");
    fs::create_dir(&outside).unwrap();
    std::os::unix::fs::symlink(&outside, root.join("Compressed/Originals")).unwrap();
    let mut snapshot = SourceSnapshot::capture(&input).unwrap();
    assert!(!move_original(&mut snapshot, &plan, &result, &AtomicBool::new(false)).moved);
    assert!(input.exists());
    assert_eq!(fs::read_dir(outside).unwrap().count(), 0);
}

#[test]
fn reports_append_all_parallel_results_and_preserve_unrelated_csv() {
    use dropper_lib::archive::{record_result, ResultRecord};
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source.mp4");
    fs::write(&source, b"source").unwrap();
    let plan = ArchivePlan::new(&source, None, None).unwrap();
    std::thread::scope(|scope| {
        for id in 0..12 {
            let plan = &plan;
            let source = &source;
            scope.spawn(move || {
                record_result(
                    plan,
                    &ResultRecord {
                        id: &id.to_string(),
                        name: "clip.mp4",
                        status: "성공",
                        source,
                        output: None,
                        archive: None,
                        message: None,
                    },
                )
                .unwrap()
            });
        }
    });
    let path = plan.base.join("Dropper-results.csv");
    let report = fs::read_to_string(&path).unwrap();
    assert_eq!(report.lines().count(), 13);
    for id in 0..12 {
        assert!(report.contains(&format!("\"{id}\",\"clip.mp4\"")));
    }
    fs::write(&path, b"USER CSV").unwrap();
    assert!(record_result(
        &plan,
        &ResultRecord {
            id: "13",
            name: "clip.mp4",
            status: "실패",
            source: &source,
            output: None,
            archive: None,
            message: None,
        }
    )
    .is_err());
    assert_eq!(fs::read(path).unwrap(), b"USER CSV");
}

#[cfg(windows)]
#[test]
fn windows_file_in_use_keeps_original_and_reports_the_saved_backup() {
    use std::os::windows::fs::OpenOptionsExt;
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("in-use.mp4");
    fs::write(&source, b"original").unwrap();
    // A player may share reads/writes while refusing deletion or renaming.
    let _player = fs::OpenOptions::new()
        .read(true)
        .share_mode(3)
        .open(&source)
        .unwrap();
    let plan = ArchivePlan::new(&source, None, None).unwrap();
    let output = plan.results_dir().unwrap().join("result.mp4");
    fs::write(&output, b"small").unwrap();
    let mut snapshot = SourceSnapshot::capture(&source).unwrap();
    let outcome = move_original(&mut snapshot, &plan, &output, &AtomicBool::new(false));
    assert!(!outcome.moved);
    assert!(outcome.warning.is_some());
    assert_eq!(fs::read(source).unwrap(), b"original");
    assert_eq!(fs::read(outcome.path.unwrap()).unwrap(), b"original");
}
