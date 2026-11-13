use dropper_lib::{
    command::{Acceleration, Settings},
    engine::Tools,
    jobs::{JobKind, Runtime},
};
use std::{collections::HashSet, fs, path::Path, process::Command, sync::atomic::Ordering};

fn inputs(root: &Path, count: usize) -> Vec<String> {
    let mut paths = vec![];
    for i in 0..count {
        let folder = root.join(i.to_string());
        fs::create_dir(&folder).unwrap();
        let path = folder.join("clip.mkv");
        if i == 0 {
            assert!(Command::new(Tools::system().unwrap().ffmpeg)
                .args([
                    "-v",
                    "error",
                    "-f",
                    "lavfi",
                    "-i",
                    "testsrc2=size=1280x720:rate=24",
                    "-t",
                    "3",
                    "-c:v",
                    "ffv1"
                ])
                .arg(&path)
                .status()
                .unwrap()
                .success());
        } else {
            fs::copy(&paths[0], &path).unwrap();
        }
        paths.push(path.to_string_lossy().into_owned());
    }
    paths
}

#[test]
fn real_parallel_batch_keeps_sources_and_same_name_outputs() {
    let temp = tempfile::tempdir().unwrap();
    let paths = inputs(temp.path(), 2);
    let original = fs::read(&paths[0]).unwrap();
    let runtime = Runtime::new(Tools::system(), "test");
    let added = runtime.add(paths.clone()).unwrap();
    let out = temp.path().join("output");
    fs::create_dir(&out).unwrap();
    let user_file = out.join("clip_compressed.mp4");
    fs::write(&user_file, b"USER FILE").unwrap();
    let mut active = HashSet::new();
    let mut peak = 0;
    let mut done = HashSet::new();
    runtime
        .work(
            added.iter().map(|file| file.id.clone()).collect(),
            Settings {
                acceleration: Acceleration::Cpu,
                parallelism: 2,
                ..Settings::default()
            },
            Some(out.to_string_lossy().into()),
            JobKind::Compress,
            1,
            &mut |event| {
                if event.phase == "processing" {
                    active.insert(event.id.clone());
                    peak = peak.max(active.len());
                } else {
                    active.remove(&event.id);
                    assert_eq!(event.phase, "done", "{:?}", event.message);
                    assert!(done.insert(event.id));
                }
            },
        )
        .unwrap();
    assert_eq!(peak, dropper_lib::acceleration::cpu_threads().min(2));
    assert_eq!(done.len(), 2);
    assert_eq!(fs::read_dir(out).unwrap().count(), 3);
    assert_eq!(fs::read(user_file).unwrap(), b"USER FILE");
    for path in paths {
        assert_eq!(fs::read(path).unwrap(), original);
    }
    assert!(!runtime.busy.load(Ordering::Acquire));
}

#[test]
fn cancellation_stops_active_and_pending_parallel_jobs_without_partial_outputs() {
    let temp = tempfile::tempdir().unwrap();
    let paths = inputs(temp.path(), 4);
    let runtime = Runtime::new(Tools::system(), "test");
    let added = runtime.add(paths).unwrap();
    let out = temp.path().join("output");
    let mut active = HashSet::new();
    let mut terminal = HashSet::new();
    let limit = dropper_lib::acceleration::cpu_threads().min(2);
    runtime
        .work(
            added.iter().map(|file| file.id.clone()).collect(),
            Settings {
                acceleration: Acceleration::Cpu,
                parallelism: 2,
                ..Settings::default()
            },
            Some(out.to_string_lossy().into()),
            JobKind::Compress,
            2,
            &mut |event| {
                if event.phase == "processing" {
                    active.insert(event.id.clone());
                    if active.len() == limit {
                        runtime.cancel.store(true, Ordering::Release);
                    }
                } else {
                    assert_eq!(event.phase, "cancelled");
                    assert!(terminal.insert(event.id));
                }
            },
        )
        .unwrap();
    assert_eq!(terminal.len(), 4);
    assert!(!out.exists() || fs::read_dir(&out).unwrap().count() == 0);
    assert!(!runtime.busy.load(Ordering::Acquire));
}
