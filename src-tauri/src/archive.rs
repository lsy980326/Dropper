//! Move verified originals without overwriting files; keep an intent receipt before removal.
use crate::folder::{self, FolderSource};
use same_file::Handle;
use serde::Serialize;
use std::{
    fs::{self, File, FileTimes},
    io::{Read, Seek, SeekFrom, Write},
    path::{Component, Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
    time::SystemTime,
};

static DIRECTORIES: Mutex<()> = Mutex::new(());
#[derive(Clone, Debug)]
pub struct ArchivePlan {
    pub base: PathBuf,
    anchor: PathBuf,
    relative: PathBuf,
}
impl ArchivePlan {
    pub fn new(
        source: &Path,
        folder: Option<&FolderSource>,
        custom: Option<&Path>,
    ) -> Result<Self, String> {
        let parent = source
            .parent()
            .ok_or("원본 폴더를 확인할 수 없습니다.")?
            .canonicalize()
            .map_err(|e| e.to_string())?;
        let anchor = custom
            .map(Path::canonicalize)
            .transpose()
            .map_err(|e| e.to_string())?
            .unwrap_or_else(|| folder.map(|s| s.root.clone()).unwrap_or(parent.clone()));
        let base = if let Some(folder) = folder {
            folder.output_base(custom)?
        } else {
            custom
                .map(Path::to_path_buf)
                .unwrap_or_else(|| parent.join("Compressed"))
        };
        let relative = folder
            .map(|s| s.relative_path.clone())
            .unwrap_or_else(|| PathBuf::from(source.file_name().unwrap_or_default()));
        if relative.as_os_str().is_empty()
            || relative
                .components()
                .any(|c| !matches!(c, Component::Normal(_)))
        {
            return Err("원본 상대 경로가 올바르지 않습니다.".into());
        }
        Ok(Self {
            base,
            anchor,
            relative,
        })
    }
    fn directory(&self, name: &str) -> Result<PathBuf, String> {
        let _guard = DIRECTORIES
            .lock()
            .map_err(|_| "폴더 준비를 잠그지 못했습니다.")?;
        let root = folder::safe_directory(&self.anchor, &self.base.join(name))?;
        if !folder::is_output(&root) {
            if fs::read_dir(&root)
                .map_err(|e| e.to_string())?
                .next()
                .is_some()
            {
                return Err(format!(
                    "{} 폴더에 기존 내용이 있습니다. 다른 저장 위치를 선택해 주세요.",
                    root.display()
                ));
            }
            folder::mark_output(&root)?;
        }
        folder::safe_directory(
            &root,
            &root.join(self.relative.parent().unwrap_or(Path::new(""))),
        )
    }
    pub fn results_dir(&self) -> Result<PathBuf, String> {
        self.directory("Results")
    }
    pub fn originals_dir(&self) -> Result<PathBuf, String> {
        self.directory("Originals")
    }
    fn records_dir(&self) -> Result<PathBuf, String> {
        let _guard = DIRECTORIES
            .lock()
            .map_err(|_| "기록 폴더를 잠그지 못했습니다.")?;
        let path = folder::safe_directory(&self.anchor, &self.base.join(".dropper-records"))?;
        folder::mark_output(&path)?;
        Ok(path)
    }
}

pub struct SourceSnapshot {
    path: PathBuf,
    handle: Option<Handle>,
    bytes: u64,
    modified: Option<SystemTime>,
}
impl SourceSnapshot {
    pub fn capture(path: &Path) -> Result<Self, String> {
        let meta = fs::symlink_metadata(path).map_err(|e| e.to_string())?;
        if !meta.is_file() || meta.file_type().is_symlink() {
            return Err("원본이 일반 파일이 아닙니다.".into());
        }
        let handle = Handle::from_file(File::open(path).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        let result = Self {
            path: path.to_path_buf(),
            handle: Some(handle),
            bytes: meta.len(),
            modified: meta.modified().ok(),
        };
        result.unchanged()?;
        Ok(result)
    }
    fn unchanged(&self) -> Result<(), String> {
        self.unchanged_at(&self.path)
    }
    fn unchanged_at(&self, path: &Path) -> Result<(), String> {
        let meta = fs::symlink_metadata(path)
            .map_err(|_| "원본 위치가 바뀌어 자동 보관하지 않았습니다.")?;
        let live = self
            .handle
            .as_ref()
            .ok_or("원본 핸들이 이미 닫혔습니다.")?
            .as_file()
            .metadata()
            .map_err(|e| e.to_string())?;
        let current = Handle::from_path(path).map_err(|e| e.to_string())?;
        if meta.file_type().is_symlink()
            || current != *self.handle.as_ref().ok_or("원본 핸들이 이미 닫혔습니다.")?
            || meta.len() != self.bytes
            || live.len() != self.bytes
            || meta.modified().ok() != self.modified
            || live.modified().ok() != self.modified
        {
            return Err(
                "원본이 변경되어 자동 이동하지 않았습니다. 압축 결과와 원본을 확인해 주세요."
                    .into(),
            );
        }
        Ok(())
    }
}
#[derive(Default, Debug)]
pub struct ArchiveOutcome {
    pub path: Option<PathBuf>,
    pub moved: bool,
    pub warning: Option<String>,
    pub receipt: Option<PathBuf>,
}
fn cancelled(cancel: &AtomicBool) -> Result<(), String> {
    if cancel.load(Ordering::Acquire) {
        Err("원본 보관이 취소되어 원본을 제자리에 유지합니다.".into())
    } else {
        Ok(())
    }
}
fn sync_dir(path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        File::open(path)
            .and_then(|f| f.sync_all())
            .map_err(|e| format!("폴더 저장 확인 실패: {e}"))?;
    }
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}
fn copied_original(
    snapshot: &mut SourceSnapshot,
    dir: &Path,
    cancel: &AtomicBool,
) -> Result<tempfile::NamedTempFile, String> {
    snapshot.unchanged()?;
    let mut temp = tempfile::Builder::new()
        .prefix(".dropper-original-")
        .tempfile_in(dir)
        .map_err(|e| e.to_string())?;
    let source = snapshot
        .handle
        .as_mut()
        .ok_or("원본 핸들이 이미 닫혔습니다.")?
        .as_file_mut();
    source.seek(SeekFrom::Start(0)).map_err(|e| e.to_string())?;
    let mut buffer = vec![0u8; 1024 * 1024];
    loop {
        cancelled(cancel)?;
        let n = source.read(&mut buffer).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        temp.write_all(&buffer[..n])
            .map_err(|e| format!("원본 보관 복사 실패: {e}"))?;
    }
    source.seek(SeekFrom::Start(0)).map_err(|e| e.to_string())?;
    temp.seek(SeekFrom::Start(0)).map_err(|e| e.to_string())?;
    let mut check = vec![0u8; buffer.len()];
    loop {
        cancelled(cancel)?;
        let n = source.read(&mut buffer).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        temp.read_exact(&mut check[..n])
            .map_err(|e| e.to_string())?;
        if buffer[..n] != check[..n] {
            return Err("원본 보관 사본의 내용이 달라 원본을 유지합니다.".into());
        }
    }
    if temp.as_file().metadata().map_err(|e| e.to_string())?.len() != snapshot.bytes {
        return Err("원본 보관 사본 크기가 다릅니다.".into());
    }
    let metadata = source.metadata().map_err(|e| e.to_string())?;
    temp.as_file()
        .set_permissions(metadata.permissions())
        .map_err(|e| e.to_string())?;
    if let Some(time) = snapshot.modified {
        temp.as_file()
            .set_times(FileTimes::new().set_modified(time))
            .map_err(|e| e.to_string())?;
    }
    temp.as_file().sync_all().map_err(|e| e.to_string())?;
    snapshot.unchanged()?;
    Ok(temp)
}
fn collision_name(original: &Path, n: usize) -> PathBuf {
    if n == 0 {
        return original.to_path_buf();
    }
    let stem = original.file_stem().unwrap_or_default().to_string_lossy();
    let extension = original
        .extension()
        .map(|s| format!(".{}", s.to_string_lossy()))
        .unwrap_or_default();
    PathBuf::from(format!("{stem}_{n}{extension}"))
}
pub fn move_original(
    snapshot: &mut SourceSnapshot,
    plan: &ArchivePlan,
    output: &Path,
    cancel: &AtomicBool,
) -> ArchiveOutcome {
    let mut outcome = ArchiveOutcome::default();
    let mut staged_location: Option<PathBuf> = None;
    let attempt = (|| -> Result<(), String> {
        cancelled(cancel)?;
        snapshot.unchanged()?;
        let results = plan.results_dir()?;
        let output = output.canonicalize().map_err(|e| e.to_string())?;
        if !output.starts_with(&results)
            || !output.is_file()
            || Handle::from_path(&output).map_err(|e| e.to_string())?
                == *snapshot
                    .handle
                    .as_ref()
                    .ok_or("원본 핸들이 이미 닫혔습니다.")?
        {
            return Err("검증된 압축 결과 위치를 확인하지 못해 원본을 유지합니다.".into());
        }
        let dir = plan.originals_dir()?;
        let filename = plan.relative.file_name().ok_or("원본 파일명 확인 실패")?;
        let mut copy = None;
        let mut backup_handle = None;
        let mut copied = false;
        for n in 0..10_000 {
            cancelled(cancel)?;
            snapshot.unchanged()?;
            let target = dir.join(collision_name(Path::new(filename), n));
            match fs::hard_link(&snapshot.path, &target) {
                Ok(()) => {
                    let handle = Handle::from_path(&target).map_err(|e| e.to_string())?;
                    if handle
                        != *snapshot
                            .handle
                            .as_ref()
                            .ok_or("원본 핸들이 이미 닫혔습니다.")?
                    {
                        return Err("보관 대상이 원본과 달라 원본을 유지합니다.".into());
                    }
                    backup_handle = Some(handle);
                    outcome.path = Some(target);
                    break;
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(_) => {
                    if copy.is_none() {
                        copy = Some(copied_original(snapshot, &dir, cancel)?);
                    }
                    let pending = copy.take().ok_or("원본 사본 준비 실패")?;
                    match pending.persist_noclobber(&target) {
                        Ok(file) => {
                            file.sync_all().map_err(|e| e.to_string())?;
                            backup_handle =
                                Some(Handle::from_file(file).map_err(|e| e.to_string())?);
                            copied = true;
                            outcome.path = Some(target);
                            break;
                        }
                        Err(e) if e.error.kind() == std::io::ErrorKind::AlreadyExists => {
                            copy = Some(e.file);
                            continue;
                        }
                        Err(e) => return Err(format!("원본 사본 저장 실패: {}", e.error)),
                    }
                }
            }
        }
        let target = outcome
            .path
            .as_ref()
            .ok_or("원본 보관 이름 충돌이 너무 많습니다.")?;
        sync_dir(&dir)?;
        let mut receipt = tempfile::Builder::new()
            .prefix("job-")
            .suffix(".jsonl")
            .tempfile_in(plan.records_dir()?)
            .map_err(|e| e.to_string())?;
        let staging = tempfile::Builder::new()
            .prefix(".dropper-stage-")
            .tempdir_in(snapshot.path.parent().ok_or("원본 폴더 확인 실패")?)
            .map_err(|e| e.to_string())?;
        let stage_dir = staging.keep();
        staged_location = Some(stage_dir.clone());
        folder::mark_output(&stage_dir)?;
        let stage = stage_dir.join(snapshot.path.file_name().ok_or("원본 파일명 확인 실패")?);
        let intent = serde_json::json!({"status":"archive_prepared","source":snapshot.path,"archive":target,"output":output,"staging":stage,"bytes":snapshot.bytes});
        serde_json::to_writer(&mut receipt, &intent).map_err(|e| e.to_string())?;
        receipt.write_all(b"\n").map_err(|e| e.to_string())?;
        receipt.as_file().sync_all().map_err(|e| e.to_string())?;
        let (mut log, receipt_path) = receipt.keep().map_err(|e| e.to_string())?;
        outcome.receipt = Some(receipt_path);
        sync_dir(&plan.records_dir()?)?;
        cancelled(cancel)?;
        snapshot.unchanged()?;
        if let Err(error) = fs::rename(&snapshot.path, &stage) {
            let _ = cleanup_stage(&stage_dir);
            return Err(format!(
                "보관 사본은 저장했지만 원본 위치에서 이동하지 못했습니다: {error}"
            ));
        }
        let checked = snapshot.unchanged_at(&stage).and_then(|_| {
            verify_backup(
                snapshot,
                target,
                backup_handle.as_mut().ok_or("보관 대상 핸들 확인 실패")?,
                copied,
                &stage,
                cancel,
            )
        });
        if let Err(error) = checked {
            drop(snapshot.handle.take());
            drop(backup_handle.take());
            let restored = restore_stage(&stage, &snapshot.path);
            if restored.is_ok() {
                let _ = cleanup_stage(&stage_dir);
            }
            return Err(format!(
                "{error} 원본 복구 결과: {}",
                restored
                    .map(|_| "원래 위치로 복구".into())
                    .unwrap_or_else(|e| format!("{e}; 보존 위치: {}", stage.display()))
            ));
        }
        outcome.moved = true;
        // Release our handles before removing the stage: Windows defers deletion of open files.
        drop(snapshot.handle.take());
        drop(backup_handle.take());
        fs::remove_file(&stage).map_err(|e| {
            format!(
                "원본 보관은 완료했지만 임시 사본 정리에 실패했습니다: {e}; {}",
                stage.display()
            )
        })?;
        cleanup_stage(&stage_dir)?;
        sync_dir(snapshot.path.parent().ok_or("원본 폴더 확인 실패")?)?;
        log.write_all(b"{\"status\":\"archived\"}\n")
            .and_then(|_| log.sync_all())
            .map_err(|e| format!("원본 보관은 완료했지만 완료 기록 저장에 실패했습니다: {e}"))?;
        Ok(())
    })();
    if let Err(mut error) = attempt {
        if let Some(directory) = staged_location {
            if directory.exists()
                && fs::read_dir(&directory).is_ok_and(|entries| {
                    entries
                        .into_iter()
                        .all(|entry| entry.is_ok_and(|e| e.file_name() == folder::OUTPUT_MARKER))
                })
            {
                if let Err(cleanup) = cleanup_stage(&directory) {
                    error.push_str(&format!(" · {cleanup}"));
                }
            }
        }
        outcome.warning = Some(error);
    }
    outcome
}

fn verify_backup(
    snapshot: &mut SourceSnapshot,
    target: &Path,
    expected: &mut Handle,
    copied: bool,
    source_path: &Path,
    cancel: &AtomicBool,
) -> Result<(), String> {
    cancelled(cancel)?;
    verify_backup_identity(snapshot, target, expected)?;
    if !copied {
        if *expected
            != *snapshot
                .handle
                .as_ref()
                .ok_or("원본 핸들이 이미 닫혔습니다.")?
        {
            return Err("보관 대상 파일이 원본과 다릅니다.".into());
        }
    } else {
        let source = snapshot
            .handle
            .as_mut()
            .ok_or("원본 핸들이 이미 닫혔습니다.")?
            .as_file_mut();
        let backup = expected.as_file_mut();
        source.seek(SeekFrom::Start(0)).map_err(|e| e.to_string())?;
        backup.seek(SeekFrom::Start(0)).map_err(|e| e.to_string())?;
        let mut a = vec![0u8; 1024 * 1024];
        let mut b = vec![0u8; a.len()];
        loop {
            cancelled(cancel)?;
            let n = source.read(&mut a).map_err(|e| e.to_string())?;
            if n == 0 {
                break;
            }
            backup.read_exact(&mut b[..n]).map_err(|e| e.to_string())?;
            if a[..n] != b[..n] {
                return Err("보관 사본의 내용이 달라 원본을 보존합니다.".into());
            }
        }
    }
    // A cross-volume comparison can take minutes: recheck after reading, before deleting.
    cancelled(cancel)?;
    snapshot.unchanged_at(source_path)?;
    verify_backup_identity(snapshot, target, expected)?;
    Ok(())
}
fn verify_backup_identity(
    snapshot: &SourceSnapshot,
    target: &Path,
    expected: &Handle,
) -> Result<(), String> {
    let metadata =
        fs::symlink_metadata(target).map_err(|_| "보관 대상이 없어 원본을 보존합니다.")?;
    let current = Handle::from_path(target).map_err(|e| e.to_string())?;
    if metadata.file_type().is_symlink()
        || current != *expected
        || metadata.len() != snapshot.bytes
        || metadata.modified().ok() != snapshot.modified
    {
        return Err("보관 대상이 변경되어 원본을 보존합니다.".into());
    }
    let held = expected.as_file().metadata().map_err(|e| e.to_string())?;
    if held.len() != snapshot.bytes || held.modified().ok() != snapshot.modified {
        return Err("보관 사본이 변경되어 원본을 보존합니다.".into());
    }
    Ok(())
}
fn cleanup_stage(directory: &Path) -> Result<(), String> {
    fs::remove_file(directory.join(folder::OUTPUT_MARKER)).map_err(|e| e.to_string())?;
    fs::remove_dir(directory).map_err(|e| format!("원본 보관 임시 폴더 정리 실패: {e}"))
}
fn restore_stage(stage: &Path, original: &Path) -> Result<(), String> {
    let mut snapshot = SourceSnapshot::capture(stage)?;
    let (mut restored, copied) = match fs::hard_link(stage, original) {
        Ok(()) => (
            Handle::from_path(original).map_err(|e| e.to_string())?,
            false,
        ),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            return Err("원래 경로에 다른 파일이 있어 덮어쓰지 않았습니다.".into())
        }
        Err(_) => {
            let pending = copied_original(
                &mut snapshot,
                original.parent().ok_or("원래 폴더 확인 실패")?,
                &AtomicBool::new(false),
            )?;
            let file = pending
                .persist_noclobber(original)
                .map_err(|e| e.error.to_string())?;
            (Handle::from_file(file).map_err(|e| e.to_string())?, true)
        }
    };
    snapshot.unchanged()?;
    verify_backup(
        &mut snapshot,
        original,
        &mut restored,
        copied,
        stage,
        &AtomicBool::new(false),
    )?;
    drop(snapshot.handle.take());
    drop(restored);
    fs::remove_file(stage).map_err(|e| e.to_string())
}

#[derive(Serialize)]
pub struct ResultRecord<'a> {
    pub id: &'a str,
    pub name: &'a str,
    pub status: &'a str,
    pub source: &'a Path,
    pub output: Option<&'a str>,
    pub archive: Option<&'a str>,
    pub message: Option<&'a str>,
}
/// Locked append keeps batch results readable even after the app has closed.
pub fn record_result(plan: &ArchivePlan, record: &ResultRecord<'_>) -> Result<PathBuf, String> {
    folder::safe_directory(&plan.anchor, &plan.base)?;
    let path = plan.base.join("Dropper-results.csv");
    if fs::symlink_metadata(&path).is_ok_and(|m| !m.is_file() || m.file_type().is_symlink()) {
        return Err("작업 기록 경로가 일반 파일이 아닙니다.".into());
    }
    let mut file = fs::OpenOptions::new()
        .read(true)
        .append(true)
        .create(true)
        .open(&path)
        .map_err(|e| e.to_string())?;
    let identity = Handle::from_file(file.try_clone().map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    if fs::symlink_metadata(&path)
        .map_err(|e| e.to_string())?
        .file_type()
        .is_symlink()
        || Handle::from_path(&path).map_err(|e| e.to_string())? != identity
    {
        return Err("작업 기록 경로가 변경되었습니다.".into());
    }
    file.lock().map_err(|e| e.to_string())?;
    let before = file.metadata().map_err(|e| e.to_string())?.len();
    let header = "\u{feff}ID,파일,상태,원본 위치,결과 위치,보관 위치,안내\r\n";
    if before > 0 {
        let mut prefix = vec![0u8; header.len()];
        if file.read_exact(&mut prefix).is_err() || prefix != header.as_bytes() {
            let _ = file.unlock();
            return Err("같은 이름의 다른 기록 파일이 있어 덮어쓰지 않았습니다.".into());
        }
    }
    let write = (|| -> Result<(), String> {
        if file.metadata().map_err(|e| e.to_string())?.len() == 0 {
            file.write_all(
                "\u{feff}ID,파일,상태,원본 위치,결과 위치,보관 위치,안내\r\n".as_bytes(),
            )
            .map_err(|e| e.to_string())?;
        }
        let source = record.source.to_string_lossy();
        let cells = [
            record.id,
            record.name,
            record.status,
            &source,
            record.output.unwrap_or(""),
            record.archive.unwrap_or(""),
            record.message.unwrap_or(""),
        ];
        let line = cells
            .iter()
            .map(|s| csv_cell(s))
            .collect::<Vec<_>>()
            .join(",")
            + "\r\n";
        file.write_all(line.as_bytes())
            .and_then(|_| file.sync_all())
            .map_err(|e| e.to_string())
    })();
    if write.is_err() {
        file.set_len(before)
            .map_err(|e| format!("작업 기록 저장·복구 실패: {e}"))?;
    }
    let unlock = file.unlock().map_err(|e| e.to_string());
    write?;
    unlock?;
    Ok(path)
}
fn csv_cell(value: &str) -> String {
    let escaped = value.replace('"', "\"\"");
    let safe = if value.trim_start().starts_with(['=', '+', '-', '@'])
        || value.starts_with(['\t', '\r', '\n'])
    {
        format!("'{escaped}")
    } else {
        escaped
    };
    format!("\"{safe}\"")
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn copy_fallback_verifies_bytes_and_keeps_source() {
        let t = tempfile::tempdir().unwrap();
        let path = t.path().join("source");
        fs::write(&path, b"original bytes").unwrap();
        let mut snapshot = SourceSnapshot::capture(&path).unwrap();
        let copy = copied_original(&mut snapshot, t.path(), &AtomicBool::new(false)).unwrap();
        assert_eq!(fs::read(copy.path()).unwrap(), b"original bytes");
        assert!(path.exists());
    }
    #[test]
    fn spreadsheet_cells_cannot_start_formulas() {
        assert_eq!(csv_cell("=1+1"), "\"'=1+1\"");
        assert_eq!(csv_cell("a\"b"), "\"a\"\"b\"");
    }
    #[test]
    fn replaced_backup_and_cancelled_verification_keep_the_staged_source() {
        let t = tempfile::tempdir().unwrap();
        let stage = t.path().join("stage");
        let target = t.path().join("backup");
        fs::write(&stage, b"original").unwrap();
        let mut snapshot = SourceSnapshot::capture(&stage).unwrap();
        fs::hard_link(&stage, &target).unwrap();
        let mut expected = Handle::from_path(&target).unwrap();
        assert!(verify_backup(
            &mut snapshot,
            &target,
            &mut expected,
            false,
            &stage,
            &AtomicBool::new(true)
        )
        .is_err());
        fs::rename(&target, t.path().join("old-backup")).unwrap();
        fs::write(&target, b"replaced").unwrap();
        assert!(verify_backup(
            &mut snapshot,
            &target,
            &mut expected,
            false,
            &stage,
            &AtomicBool::new(false)
        )
        .is_err());
        assert_eq!(fs::read(&stage).unwrap(), b"original");
        let original = t.path().join("original");
        fs::write(&original, b"USER FILE").unwrap();
        assert!(restore_stage(&stage, &original).is_err());
        assert_eq!(fs::read(&stage).unwrap(), b"original");
        assert_eq!(fs::read(&original).unwrap(), b"USER FILE");
    }
    #[test]
    fn cancelled_copy_does_not_publish_or_remove_a_source() {
        let t = tempfile::tempdir().unwrap();
        let source = t.path().join("source");
        fs::write(&source, b"original").unwrap();
        let mut snapshot = SourceSnapshot::capture(&source).unwrap();
        assert!(copied_original(&mut snapshot, t.path(), &AtomicBool::new(true)).is_err());
        assert_eq!(fs::read(&source).unwrap(), b"original");
        assert_eq!(fs::read_dir(t.path()).unwrap().count(), 1);
    }
}
