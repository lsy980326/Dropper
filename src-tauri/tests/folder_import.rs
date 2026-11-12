use dropper_lib::{
    command::{Codec, Settings},
    engine::Tools,
    jobs::{JobKind, Runtime},
};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

fn library(base: &Path) -> (PathBuf, Vec<u8>) {
    let template = base.join("template.mkv");
    let output = Command::new(Tools::system().unwrap().ffmpeg)
        .args([
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=160x96:rate=12",
            "-t",
            "1",
            "-c:v",
            "ffv1",
        ])
        .arg(&template)
        .output()
        .unwrap();
    assert!(output.status.success());
    let data = fs::read(template).unwrap();
    let root = base.join("내 영상");
    for name in ["2024/clip.mkv", "2025/clip.MKV"] {
        let path = root.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, &data).unwrap();
    }
    fs::create_dir(root.join("Compressed")).unwrap();
    fs::write(root.join("Compressed/existing.mp4"), b"USER RESULT").unwrap();
    fs::write(root.join("notes.txt"), b"KEEP").unwrap();
    (root.canonicalize().unwrap(), data)
}

#[test]
fn folder_batch_retains_subdirectories_and_originals() {
    let temp = tempfile::tempdir().unwrap();
    let (root, original) = library(temp.path());
    let runtime = Runtime::new(Tools::system(), "system");
    let mut updates = vec![];
    let result = runtime
        .import_paths(
            vec![root.to_string_lossy().into_owned()],
            None,
            7,
            &mut |p| updates.push(p),
        )
        .unwrap();
    assert_eq!(result.files.len(), 2);
    assert_eq!(result.skipped_results, 1);
    assert!(updates.iter().any(|p| p.run_id == 7 && p.checked == 2));
    let mut events = vec![];
    runtime
        .work(
            result.files.iter().map(|f| f.id.clone()).collect(),
            Settings {
                codec: Codec::H264,
                ..Settings::default()
            },
            None,
            JobKind::Compress,
            8,
            &mut |e| events.push(e),
        )
        .unwrap();
    assert_eq!(events.iter().filter(|e| e.phase == "done").count(), 2);
    for relative in ["2024/clip_compressed.mp4", "2025/clip_compressed.mp4"] {
        assert!(root.join("Compressed").join(relative).is_file());
    }
    assert_eq!(fs::read(root.join("2024/clip.mkv")).unwrap(), original);
    assert_eq!(fs::read(root.join("2025/clip.MKV")).unwrap(), original);
    assert_eq!(
        fs::read(root.join("Compressed/existing.mp4")).unwrap(),
        b"USER RESULT"
    );
    assert_eq!(fs::read(root.join("notes.txt")).unwrap(), b"KEEP");
}

#[test]
fn folder_can_be_dropped_with_files_and_custom_destination_preserves_root() {
    let temp = tempfile::tempdir().unwrap();
    let (root, _) = library(temp.path());
    let destination = temp.path().join("exports");
    fs::create_dir(&destination).unwrap();
    let destination = destination.canonicalize().unwrap();
    let runtime = Runtime::new(Tools::system(), "system");
    let first = runtime
        .add(vec![root
            .join("2024/clip.mkv")
            .to_string_lossy()
            .into_owned()])
        .unwrap();
    let imported = runtime
        .import_paths(
            vec![
                root.join("2024/clip.mkv").to_string_lossy().into_owned(),
                root.to_string_lossy().into_owned(),
            ],
            Some(destination.to_string_lossy().into_owned()),
            1,
            &mut |_| {},
        )
        .unwrap();
    assert_eq!(imported.files.len(), 2);
    assert!(imported.files.iter().any(|f| f.id == first[0].id));
    runtime
        .work(
            imported.files.iter().map(|f| f.id.clone()).collect(),
            Settings {
                codec: Codec::H264,
                ..Settings::default()
            },
            Some(destination.to_string_lossy().into_owned()),
            JobKind::Compress,
            2,
            &mut |_| {},
        )
        .unwrap();
    assert!(destination
        .join("내 영상/2024/clip_compressed.mp4")
        .is_file());
    assert!(destination
        .join("내 영상/2025/clip_compressed.mp4")
        .is_file());
}

#[test]
fn choosing_source_parent_as_destination_does_not_hide_nested_sources() {
    let temp = tempfile::tempdir().unwrap();
    let (root, _) = library(temp.path());
    let destination = temp.path().canonicalize().unwrap();
    let runtime = Runtime::new(Tools::system(), "system");
    let imported = runtime
        .import_paths(
            vec![root.to_string_lossy().into_owned()],
            Some(destination.to_string_lossy().into_owned()),
            1,
            &mut |_| {},
        )
        .unwrap();
    assert_eq!(imported.files.len(), 2);
    runtime
        .work(
            imported.files.iter().map(|f| f.id.clone()).collect(),
            Settings {
                codec: Codec::H264,
                ..Settings::default()
            },
            Some(destination.to_string_lossy().into_owned()),
            JobKind::Compress,
            2,
            &mut |_| {},
        )
        .unwrap();
    assert!(destination
        .join("내 영상_Compressed/2024/clip_compressed.mp4")
        .is_file());
    assert!(destination
        .join("내 영상_Compressed/2025/clip_compressed.mp4")
        .is_file());
}

#[cfg(unix)]
#[test]
fn directly_dropped_links_are_excluded_and_reported() {
    let temp = tempfile::tempdir().unwrap();
    let (root, _) = library(temp.path());
    let folder_link = temp.path().join("folder-link");
    let file_link = temp.path().join("file-link.mkv");
    std::os::unix::fs::symlink(&root, &folder_link).unwrap();
    std::os::unix::fs::symlink(root.join("2024/clip.mkv"), &file_link).unwrap();
    let runtime = Runtime::new(Tools::system(), "system");
    let result = runtime
        .import_paths(
            vec![
                folder_link.to_string_lossy().into_owned(),
                file_link.to_string_lossy().into_owned(),
            ],
            None,
            1,
            &mut |_| {},
        )
        .unwrap();
    assert!(result.files.is_empty());
    assert_eq!(result.skipped_links, 2);
}

#[test]
fn cancelling_import_returns_only_checked_files_and_releases_the_worker() {
    let temp = tempfile::tempdir().unwrap();
    let (root, _) = library(temp.path());
    let runtime = Runtime::new(Tools::system(), "system");
    let result = runtime
        .import_paths(
            vec![root.to_string_lossy().into_owned()],
            None,
            1,
            &mut |p| {
                if p.checked == 1 {
                    runtime
                        .cancel
                        .store(true, std::sync::atomic::Ordering::Release);
                }
            },
        )
        .unwrap();
    assert!(result.cancelled);
    assert_eq!(result.files.len(), 1);
    assert!(runtime.begin().is_ok());
}

#[test]
fn more_than_500_batch_entries_are_deduplicated_and_clear_preserves_saved_files() {
    let temp = tempfile::tempdir().unwrap();
    let (root, _) = library(temp.path());
    let runtime = Runtime::new(Tools::system(), "system");
    let files = runtime
        .add(vec![root
            .join("2024/clip.mkv")
            .to_string_lossy()
            .into_owned()])
        .unwrap();
    let mut events = vec![];
    runtime
        .work(
            vec![files[0].id.clone(); 601],
            Settings {
                codec: Codec::H264,
                ..Settings::default()
            },
            None,
            JobKind::Compress,
            1,
            &mut |e| events.push(e),
        )
        .unwrap();
    let completed: Vec<_> = events.iter().filter(|e| e.phase == "done").collect();
    assert_eq!(completed.len(), 1);
    let output = PathBuf::from(completed[0].output_path.as_ref().unwrap());
    runtime.clear().unwrap();
    assert!(output.is_file());
    assert!(runtime
        .work(
            vec![files[0].id.clone()],
            Settings::default(),
            None,
            JobKind::Compress,
            2,
            &mut |_| {}
        )
        .is_err());
}
