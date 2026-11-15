use std::{
    fs,
    path::{Component, Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};

#[derive(Debug, Clone, PartialEq)]
pub struct FolderSource {
    pub root: PathBuf,
    pub relative_path: PathBuf,
}

#[derive(Debug)]
pub struct FolderScan {
    pub root: PathBuf,
    pub files: Vec<PathBuf>,
    pub warnings: Vec<String>,
    pub skipped_results: usize,
    pub skipped_links: usize,
    pub ignored_files: usize,
}

pub fn scan(
    root: &Path,
    exclude: Option<&Path>,
    cancel: &AtomicBool,
    found: &mut dyn FnMut(usize),
) -> Result<FolderScan, String> {
    if fs::symlink_metadata(root).is_ok_and(|meta| meta.file_type().is_symlink()) {
        return Err("심볼릭 링크 대신 실제 영상 폴더를 선택해 주세요.".into());
    }
    let root = root
        .canonicalize()
        .map_err(|e| format!("폴더를 찾을 수 없습니다: {e}"))?;
    if !root.is_dir() {
        return Err("영상이 들어 있는 폴더를 선택해 주세요.".into());
    }
    let mut result = FolderScan {
        root: root.clone(),
        files: vec![],
        warnings: vec![],
        skipped_results: 0,
        skipped_links: 0,
        ignored_files: 0,
    };
    let mut pending = vec![root.clone()];
    while let Some(directory) = pending.pop() {
        if cancel.load(Ordering::Acquire) {
            break;
        }
        match directory.canonicalize() {
            Ok(real) if real.starts_with(&root) => {}
            _ => {
                result.warnings.push(format!(
                    "폴더 경로가 변경되어 제외했습니다: {}",
                    directory.display()
                ));
                continue;
            }
        }
        let entries = match fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(error) if directory == root => {
                return Err(format!("폴더를 읽을 수 없습니다: {error}"))
            }
            Err(error) => {
                result
                    .warnings
                    .push(format!("{}: {error}", directory.display()));
                continue;
            }
        };
        let mut entries = entries.collect::<Vec<_>>();
        entries.sort_by_key(|entry| entry.as_ref().ok().map(|e| e.path()));
        for entry in entries {
            if cancel.load(Ordering::Acquire) {
                break;
            }
            let entry = match entry {
                Ok(entry) => entry,
                Err(e) => {
                    result
                        .warnings
                        .push(format!("{}: {e}", directory.display()));
                    continue;
                }
            };
            let path = entry.path();
            let kind = match entry.file_type() {
                Ok(kind) => kind,
                Err(e) => {
                    result.warnings.push(format!("{}: {e}", path.display()));
                    continue;
                }
            };
            if kind.is_symlink() {
                result.skipped_links += 1;
                continue;
            }
            if kind.is_dir() {
                if is_output(&path)
                    || exclude.is_some_and(|base| path.starts_with(base) && is_output(base))
                {
                    result.skipped_results += 1;
                    continue;
                }
                pending.push(path);
            } else if kind.is_file() && is_video(&path) {
                match path.canonicalize() {
                    Ok(real) if real.starts_with(&root) => {
                        result.files.push(real);
                        found(result.files.len());
                    }
                    _ => result.warnings.push(format!(
                        "파일 경로가 변경되어 제외했습니다: {}",
                        path.display()
                    )),
                }
            } else if kind.is_file() {
                result.ignored_files += 1;
            }
        }
    }
    result.files.sort();
    result.files.dedup();
    Ok(result)
}

pub fn is_video(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|extension| {
            [
                "mp4", "mov", "mkv", "avi", "webm", "wmv", "flv", "mpeg", "mpg", "ts", "mts",
                "m2ts", "m4v", "3gp", "3g2", "mxf", "asf", "vob", "ogv", "m2t", "mpe", "qt", "f4v",
            ]
            .iter()
            .any(|candidate| extension.eq_ignore_ascii_case(candidate))
        })
}

impl FolderSource {
    pub fn output_base(&self, custom: Option<&Path>) -> Result<PathBuf, String> {
        match custom {
            Some(custom) => {
                let custom = custom
                    .canonicalize()
                    .map_err(|e| format!("저장 폴더를 확인할 수 없습니다: {e}"))?;
                let mut name = self
                    .root
                    .file_name()
                    .unwrap_or_else(|| std::ffi::OsStr::new("Videos"))
                    .to_os_string();
                let base = custom.join(&name);
                // Selecting the source's parent must not turn the source into its own output.
                if self.root.starts_with(&base) {
                    name.push("_Compressed");
                }
                Ok(custom.join(name))
            }
            None => Ok(self.root.join("Compressed")),
        }
    }
    pub fn output_dir(&self, custom: Option<&Path>) -> Result<PathBuf, String> {
        if self.relative_path.as_os_str().is_empty()
            || self
                .relative_path
                .components()
                .any(|c| !matches!(c, Component::Normal(_)))
        {
            return Err("폴더 안의 상대 영상 경로가 올바르지 않습니다.".into());
        }
        let base = self.output_base(custom)?;
        let anchor = match custom {
            Some(path) => path,
            None => &self.root,
        }
        .canonicalize()
        .map_err(|e| e.to_string())?;
        let destination = base.join(self.relative_path.parent().unwrap_or(Path::new("")));
        safe_directory(&anchor, &destination)
    }
}

pub const OUTPUT_MARKER: &str = ".dropper-output";
pub fn is_output(path: &Path) -> bool {
    let marker = path.join(OUTPUT_MARKER);
    fs::symlink_metadata(&marker)
        .is_ok_and(|m| m.is_file() && !m.file_type().is_symlink() && m.len() == 18)
        && fs::read(marker).is_ok_and(|bytes| bytes == b"dropper-output-v1\n")
}
pub fn mark_output(path: &Path) -> Result<(), String> {
    use std::io::Write;
    let marker = path.join(OUTPUT_MARKER);
    match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&marker)
    {
        Ok(mut f) => f
            .write_all(b"dropper-output-v1\n")
            .and_then(|_| f.sync_all())
            .map_err(|e| e.to_string()),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists && is_output(path) => Ok(()),
        Err(e) => Err(format!("결과 폴더 표시를 저장하지 못했습니다: {e}")),
    }
}

pub fn safe_directory(anchor: &Path, destination: &Path) -> Result<PathBuf, String> {
    let relative = destination
        .strip_prefix(&anchor)
        .map_err(|_| "결과 경로가 저장 폴더를 벗어났습니다.")?;
    let mut current = anchor.canonicalize().map_err(|e| e.to_string())?;
    for part in relative.components() {
        let Component::Normal(part) = part else {
            return Err("결과 경로가 올바르지 않습니다.".into());
        };
        current.push(part);
        match fs::symlink_metadata(&current){
                Ok(meta) if meta.is_dir()&&!meta.file_type().is_symlink()=>{},
                Ok(_)=>return Err("결과 폴더 경로에 파일 또는 심볼릭 링크가 있습니다. 다른 저장 위치를 선택해 주세요.".into()),
                Err(error) if error.kind()==std::io::ErrorKind::NotFound=>match fs::create_dir(&current) {
                    Ok(())=>{},
                    // Another batch worker may create the shared parent first.
                    Err(error) if error.kind()==std::io::ErrorKind::AlreadyExists=>{
                        let meta=fs::symlink_metadata(&current).map_err(|e|format!("결과 폴더를 확인하지 못했습니다: {e}"))?;
                        if !meta.is_dir()||meta.file_type().is_symlink(){return Err("결과 폴더 경로에 파일 또는 심볼릭 링크가 있습니다. 다른 저장 위치를 선택해 주세요.".into());}
                    },
                    Err(error)=>return Err(format!("결과 폴더를 만들지 못했습니다: {error}")),
                },
                Err(error)=>return Err(format!("결과 폴더를 확인하지 못했습니다: {error}")),
            }
    }
    Ok(current)
}
