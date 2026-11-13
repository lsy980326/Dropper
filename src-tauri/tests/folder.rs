use dropper_lib::folder::{scan, FolderSource};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};

fn touch(root: &Path, name: &str) {
    let path = root.join(name);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, b"fixture").unwrap();
}

#[test]
fn recursively_collects_videos_in_stable_order_and_skips_other_files_and_results() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    for name in [
        "b.MP4",
        "a.mov",
        "여행/2.MKV",
        "여행/readme.txt",
        "Compressed/old.mp4",
        "여행/Compressed/old.mov",
    ] {
        touch(&root, name);
    }
    let scan = scan(&root, None, &AtomicBool::new(false), &mut |_| {}).unwrap();
    let names: Vec<_> = scan
        .files
        .iter()
        .map(|p| p.strip_prefix(&root).unwrap().to_path_buf())
        .collect();
    assert_eq!(
        names,
        vec![
            PathBuf::from("a.mov"),
            PathBuf::from("b.MP4"),
            Path::new("여행").join("2.MKV")
        ]
    );
    assert_eq!(scan.skipped_results, 2);
    assert!(scan.warnings.is_empty());
}

#[test]
fn includes_more_than_500_files_and_can_select_a_folder_named_compressed() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("Compressed");
    fs::create_dir(&root).unwrap();
    for index in 0..601 {
        touch(&root, &format!("{index:04}.mp4"));
    }
    assert_eq!(
        scan(&root, None, &AtomicBool::new(false), &mut |_| {})
            .unwrap()
            .files
            .len(),
        601
    );
}

#[test]
fn skips_custom_destination_and_stops_discovery_on_cancel() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    for name in ["a.mp4", "b.mov", "results/root/old.mp4"] {
        touch(&root, name);
    }
    let result = scan(
        &root,
        Some(&root.join("results")),
        &AtomicBool::new(false),
        &mut |_| {},
    )
    .unwrap();
    assert_eq!(result.files.len(), 2);
    let cancel = AtomicBool::new(false);
    let result = scan(&root, None, &cancel, &mut |_| {
        cancel.store(true, Ordering::Release)
    })
    .unwrap();
    assert!(result.files.len() <= 1);
    assert!(cancel.load(Ordering::Acquire));
}

#[test]
fn output_retains_structure_and_rejects_parent_traversal() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap().join("여행");
    fs::create_dir(&root).unwrap();
    let source = FolderSource {
        root: root.clone(),
        relative_path: "2025/여름/clip.mov".into(),
    };
    assert_eq!(
        source.output_dir(None).unwrap(),
        root.join("Compressed/2025/여름")
    );
    let custom = temp.path().join("exports");
    fs::create_dir(&custom).unwrap();
    let custom = custom.canonicalize().unwrap();
    assert_eq!(
        source.output_dir(Some(&custom)).unwrap(),
        custom.join("여행/2025/여름")
    );
    let invalid = FolderSource {
        root,
        relative_path: "../escape.mov".into(),
    };
    assert!(invalid.output_dir(None).is_err());
}

#[cfg(unix)]
#[test]
fn never_follows_symbolic_links_or_output_links() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("source");
    fs::create_dir(&root).unwrap();
    let root = root.canonicalize().unwrap();
    touch(&root, "a.mp4");
    std::os::unix::fs::symlink(&root, root.join("loop")).unwrap();
    std::os::unix::fs::symlink(root.join("a.mp4"), root.join("alias.mp4")).unwrap();
    let result = scan(&root, None, &AtomicBool::new(false), &mut |_| {}).unwrap();
    assert_eq!(result.files.len(), 1);
    assert_eq!(result.skipped_links, 2);
    fs::create_dir(root.join("Compressed")).unwrap();
    let outside = temp.path().join("outside");
    fs::create_dir(&outside).unwrap();
    std::os::unix::fs::symlink(&outside, root.join("Compressed/nested")).unwrap();
    let source = FolderSource {
        root,
        relative_path: "nested/a.mp4".into(),
    };
    assert!(source.output_dir(None).is_err());
    assert_eq!(fs::read_dir(outside).unwrap().count(), 0);
}
