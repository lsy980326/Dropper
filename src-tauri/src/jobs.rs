use crate::{
    command::{output_extension, Codec, Settings},
    engine::{canonical_input, Tools},
    folder::{self, FolderSource},
    media::Media,
    process::execute,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant, SystemTime},
};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileItem {
    pub id: String,
    pub version: u64,
    pub name: String,
    pub bytes: u64,
    pub duration: f64,
    pub width: u32,
    pub height: u32,
    pub codec: String,
    pub hdr: Option<String>,
    pub audio_count: usize,
    pub subtitle_count: usize,
    pub requires_mkv: bool,
    pub blocked: Option<String>,
    pub notes: Vec<String>,
    pub folder_name: Option<String>,
    pub relative_path: Option<String>,
}
#[derive(Clone)]
struct Input {
    path: PathBuf,
    modified: Option<SystemTime>,
    item: FileItem,
    media: Media,
    folder_source: Option<FolderSource>,
}

#[derive(Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum JobKind {
    Estimate,
    Compress,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobEvent {
    pub run_id: u64,
    pub id: String,
    pub kind: JobKind,
    pub phase: String,
    pub progress: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub estimated_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remaining_seconds: Option<f64>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Environment {
    pub ready: bool,
    pub source: String,
    pub codecs: Vec<Codec>,
    pub error: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportResult {
    pub files: Vec<FileItem>,
    pub warnings: Vec<String>,
    pub skipped_results: usize,
    pub skipped_links: usize,
    pub cancelled: bool,
    pub folder_count: usize,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportProgress {
    pub run_id: u64,
    pub stage: String,
    pub found: usize,
    pub checked: usize,
}

pub struct Runtime {
    pub tools: Result<Tools, String>,
    pub source: String,
    inputs: Mutex<HashMap<String, Input>>,
    outputs: Mutex<HashMap<String, PathBuf>>,
    pub busy: AtomicBool,
    pub cancel: AtomicBool,
    next_id: AtomicU64,
}
pub struct Guard(Arc<Runtime>);
impl Drop for Guard {
    fn drop(&mut self) {
        self.0.busy.store(false, Ordering::Release);
    }
}

impl Runtime {
    pub fn import_paths(
        self: &Arc<Self>,
        paths: Vec<String>,
        output_folder: Option<String>,
        run_id: u64,
        emit: &mut dyn FnMut(ImportProgress),
    ) -> Result<ImportResult, String> {
        let _guard = self.begin()?;
        if paths.len() > 200 {
            return Err("한 번에 선택하는 파일·폴더는 최대 200개입니다. 폴더 안의 영상 수는 제한하지 않습니다.".into());
        }
        let mut candidates: Vec<(PathBuf, Option<FolderSource>)> = vec![];
        let mut positions: HashMap<PathBuf, usize> = HashMap::new();
        let mut warnings = vec![];
        let mut skipped_results = 0;
        let mut skipped_links = 0;
        let mut folder_count = 0;
        for text in paths {
            if self.cancel.load(Ordering::Acquire) {
                break;
            }
            let path = PathBuf::from(text);
            if fs::symlink_metadata(&path).is_ok_and(|meta| meta.file_type().is_symlink()) {
                skipped_links += 1;
                continue;
            }
            if path.is_dir() {
                folder_count += 1;
                let root = match path.canonicalize() {
                    Ok(root) => root,
                    Err(e) => {
                        warnings.push(format!("{}: {e}", path.display()));
                        continue;
                    }
                };
                let destination = FolderSource {
                    root: root.clone(),
                    relative_path: PathBuf::new(),
                }
                .output_base(output_folder.as_deref().map(Path::new))?;
                let previous = candidates.len();
                let mut last = Instant::now() - Duration::from_secs(1);
                let found = folder::scan(&root, Some(&destination), &self.cancel, &mut |count| {
                    if last.elapsed() >= Duration::from_millis(100) {
                        last = Instant::now();
                        emit(ImportProgress {
                            run_id,
                            stage: "scanning".into(),
                            found: previous + count,
                            checked: 0,
                        });
                    }
                });
                match found {
                    Ok(scan) => {
                        skipped_results += scan.skipped_results;
                        skipped_links += scan.skipped_links;
                        warnings.extend(scan.warnings);
                        for path in scan.files {
                            let relative_path = path
                                .strip_prefix(&scan.root)
                                .map_err(|_| "영상 경로가 선택한 폴더를 벗어났습니다.")?
                                .to_path_buf();
                            let source = FolderSource {
                                root: scan.root.clone(),
                                relative_path,
                            };
                            if let Some(index) = positions.get(&path).copied() {
                                if candidates[index]
                                    .1
                                    .as_ref()
                                    .is_none_or(|old| old.root.starts_with(&source.root))
                                {
                                    candidates[index].1 = Some(source);
                                }
                            } else {
                                positions.insert(path.clone(), candidates.len());
                                candidates.push((path, Some(source)));
                            }
                        }
                    }
                    Err(error) => warnings.push(error),
                }
            } else {
                let path = path.canonicalize().unwrap_or(path);
                if !positions.contains_key(&path) {
                    positions.insert(path.clone(), candidates.len());
                    candidates.push((path, None));
                }
            }
        }
        let total = candidates.len();
        emit(ImportProgress {
            run_id,
            stage: "analyzing".into(),
            found: total,
            checked: 0,
        });
        let mut last = Instant::now() - Duration::from_secs(1);
        let files = self.add_inputs(candidates, &mut |checked| {
            if checked != total && last.elapsed() < Duration::from_millis(120) {
                return;
            }
            last = Instant::now();
            emit(ImportProgress {
                run_id,
                stage: "analyzing".into(),
                found: total,
                checked,
            })
        })?;
        Ok(ImportResult {
            files,
            warnings,
            skipped_results,
            skipped_links,
            folder_count,
            cancelled: self.cancel.load(Ordering::Acquire),
        })
    }
    pub fn new(tools: Result<Tools, String>, source: &str) -> Arc<Self> {
        Arc::new(Self {
            tools,
            source: source.into(),
            inputs: Mutex::new(HashMap::new()),
            outputs: Mutex::new(HashMap::new()),
            busy: AtomicBool::new(false),
            cancel: AtomicBool::new(false),
            next_id: AtomicU64::new(1),
        })
    }
    pub fn begin(self: &Arc<Self>) -> Result<Guard, String> {
        self.busy
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| "다른 작업이 진행 중입니다. 완료하거나 취소한 뒤 다시 시도해 주세요.")?;
        self.cancel.store(false, Ordering::Release);
        Ok(Guard(self.clone()))
    }
    pub fn environment(&self) -> Environment {
        let result = self.tools.as_ref().map_err(Clone::clone).and_then(|tools| {
            let output = execute(
                &tools.ffmpeg,
                &["-hide_banner".into(), "-encoders".into()],
                &AtomicBool::new(false),
                Duration::from_secs(15),
                true,
                &mut |_| {},
            )?;
            let text = String::from_utf8_lossy(&output);
            let codecs = [
                (Codec::H264, "libx264"),
                (Codec::H265, "libx265"),
                (Codec::Av1, "libsvtav1"),
            ]
            .into_iter()
            .filter_map(|(codec, name)| text.split_whitespace().any(|s| s == name).then_some(codec))
            .collect::<Vec<_>>();
            if codecs.is_empty() {
                Err("지원하는 영상 인코더가 없습니다. FFmpeg 구성을 확인해 주세요.".into())
            } else {
                Ok(codecs)
            }
        });
        match result {
            Ok(codecs) => Environment {
                ready: true,
                source: self.source.clone(),
                codecs,
                error: None,
            },
            Err(error) => Environment {
                ready: false,
                source: self.source.clone(),
                codecs: vec![],
                error: Some(error),
            },
        }
    }
    pub fn add(self: &Arc<Self>, paths: Vec<String>) -> Result<Vec<FileItem>, String> {
        let _guard = self.begin()?;
        if paths.len() > 200 {
            return Err("한 번에 최대 200개까지 추가할 수 있습니다.".into());
        }
        self.add_inputs(
            paths
                .into_iter()
                .map(|path| (PathBuf::from(path), None))
                .collect(),
            &mut |_| {},
        )
    }
    fn add_inputs(
        self: &Arc<Self>,
        paths: Vec<(PathBuf, Option<FolderSource>)>,
        checked: &mut dyn FnMut(usize),
    ) -> Result<Vec<FileItem>, String> {
        let tools = self.tools.as_ref().map_err(Clone::clone)?;
        let mut result = vec![];
        for (path, folder_source) in paths {
            if self.cancel.load(Ordering::Acquire) {
                break;
            }
            let version = self.next_id.fetch_add(1, Ordering::Relaxed);
            let id = format!("f{version}");
            let name = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            let analysis = (|| {
                let path = canonical_input(&path)?;
                if let Some(source) = &folder_source {
                    if path.strip_prefix(&source.root).ok() != Some(source.relative_path.as_path())
                    {
                        return Err("추가 중 영상 경로가 선택한 폴더 밖으로 변경되었습니다.".into());
                    }
                }
                let before = fs::metadata(&path).map_err(|e| e.to_string())?;
                let inputs = self
                    .inputs
                    .lock()
                    .map_err(|_| "파일 목록을 잠그지 못했습니다.")?;
                let previous = inputs.values().find(|i| i.path == path).cloned();
                let source = folder_source
                    .clone()
                    .or_else(|| previous.as_ref().and_then(|i| i.folder_source.clone()));
                if let Some(input) = &previous {
                    if input.media.bytes == before.len()
                        && input.modified == before.modified().ok()
                        && input.folder_source == source
                    {
                        return Ok(input.item.clone());
                    }
                }
                let actual_id = previous
                    .as_ref()
                    .map(|i| i.item.id.clone())
                    .unwrap_or_else(|| id.clone());
                drop(inputs);
                let media = tools.probe(&path, &self.cancel)?;
                let meta = fs::metadata(&path).map_err(|e| e.to_string())?;
                if before.len() != meta.len() || before.modified().ok() != meta.modified().ok() {
                    return Err(
                        "분석 중 원본이 변경되었습니다. 저장을 마친 뒤 다시 추가해 주세요.".into(),
                    );
                }
                let mut notes = vec![];
                if media.requires_mkv() {
                    notes.push("원본 트랙 보존을 위해 MKV로 저장합니다.".into());
                }
                if media.video.width.unwrap_or(0) % 2 == 1
                    || media.video.height.unwrap_or(0) % 2 == 1
                {
                    notes.push("인코더 호환성을 위해 가장자리에 최대 1픽셀을 추가합니다.".into());
                }
                if !media.video.pix_fmt.starts_with("yuv420p") {
                    notes.push("호환성을 위해 색 표본 형식을 4:2:0으로 변환합니다.".into());
                }
                let item = FileItem {
                    id: actual_id.clone(),
                    version,
                    name: name.clone(),
                    bytes: media.bytes,
                    duration: media.duration,
                    width: media.video.width.unwrap_or(0),
                    height: media.video.height.unwrap_or(0),
                    codec: media.video.codec_name.clone(),
                    hdr: media.hdr.clone(),
                    audio_count: media
                        .streams
                        .iter()
                        .filter(|s| s.codec_type == "audio")
                        .count(),
                    subtitle_count: media
                        .streams
                        .iter()
                        .filter(|s| s.codec_type == "subtitle")
                        .count(),
                    requires_mkv: media.requires_mkv(),
                    blocked: media.blocked.clone(),
                    notes,
                    folder_name: source
                        .as_ref()
                        .and_then(|s| s.root.file_name())
                        .map(|s| s.to_string_lossy().into_owned()),
                    relative_path: source
                        .as_ref()
                        .map(|s| s.relative_path.to_string_lossy().into_owned()),
                };
                self.inputs
                    .lock()
                    .map_err(|_| "파일 목록을 잠그지 못했습니다.")?
                    .insert(
                        actual_id,
                        Input {
                            path,
                            modified: meta.modified().ok(),
                            item: item.clone(),
                            media,
                            folder_source: source,
                        },
                    );
                Ok::<_, String>(item)
            })();
            result.push(analysis.unwrap_or_else(|reason| {
                FileItem {
                    id,
                    version,
                    name,
                    bytes: 0,
                    duration: 0.0,
                    width: 0,
                    height: 0,
                    codec: String::new(),
                    hdr: None,
                    audio_count: 0,
                    subtitle_count: 0,
                    requires_mkv: false,
                    blocked: Some(reason),
                    notes: vec![],
                    folder_name: folder_source
                        .as_ref()
                        .and_then(|s| s.root.file_name())
                        .map(|s| s.to_string_lossy().into_owned()),
                    relative_path: folder_source
                        .as_ref()
                        .map(|s| s.relative_path.to_string_lossy().into_owned()),
                }
            }));
            checked(result.len());
        }
        Ok(result)
    }
    pub fn remove(self: &Arc<Self>, id: &str) -> Result<(), String> {
        let _guard = self.begin()?;
        self.inputs
            .lock()
            .map_err(|_| "파일 목록을 잠그지 못했습니다.")?
            .remove(id);
        self.outputs
            .lock()
            .map_err(|_| "결과 목록을 잠그지 못했습니다.")?
            .remove(id);
        Ok(())
    }
    pub fn clear(self: &Arc<Self>) -> Result<(), String> {
        let _guard = self.begin()?;
        let mut inputs = self
            .inputs
            .lock()
            .map_err(|_| "파일 목록을 잠그지 못했습니다.")?;
        let mut outputs = self
            .outputs
            .lock()
            .map_err(|_| "결과 목록을 잠그지 못했습니다.")?;
        inputs.clear();
        outputs.clear();
        Ok(())
    }
    pub fn output_folder(&self, id: &str) -> Result<PathBuf, String> {
        self.outputs
            .lock()
            .map_err(|_| "결과 목록을 잠그지 못했습니다.")?
            .get(id)
            .and_then(|p| p.parent())
            .map(Path::to_path_buf)
            .ok_or_else(|| "저장된 결과가 없습니다.".into())
    }
    pub fn work(
        self: &Arc<Self>,
        ids: Vec<String>,
        settings: Settings,
        folder: Option<String>,
        kind: JobKind,
        run_id: u64,
        emit: &mut dyn FnMut(JobEvent),
    ) -> Result<(), String> {
        let _guard = self.begin()?;
        let tools = self.tools.as_ref().map_err(Clone::clone)?;
        let inputs = self
            .inputs
            .lock()
            .map_err(|_| "파일 목록을 잠그지 못했습니다.")?;
        let mut seen = HashSet::new();
        let selected = ids
            .into_iter()
            .filter(|id| seen.insert(id.clone()))
            .map(|id| {
                inputs.get(&id).cloned().ok_or_else(|| {
                    "목록에 등록되지 않은 영상입니다. 다시 추가해 주세요.".to_string()
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        drop(inputs);
        for input in selected {
            let mut event = JobEvent {
                run_id,
                id: input.item.id.clone(),
                kind,
                phase: "processing".into(),
                progress: 0.0,
                estimated_bytes: None,
                output_bytes: None,
                output_path: None,
                message: None,
                remaining_seconds: None,
            };
            if self.cancel.load(Ordering::Acquire) {
                event.phase = "cancelled".into();
                event.message = Some("취소됨".into());
                emit(event);
                continue;
            }
            emit(event.clone());
            let result = (|| {
                let meta = fs::metadata(&input.path)
                    .map_err(|_| "원본을 찾을 수 없습니다. 파일을 다시 추가해 주세요.")?;
                if meta.len() != input.media.bytes || meta.modified().ok() != input.modified {
                    return Err(
                        "추가한 뒤 원본이 변경되었습니다. 파일을 다시 추가해 주세요.".into(),
                    );
                }
                output_extension(&input.media, &settings)?;
                let start = Instant::now();
                let mut last = Instant::now() - Duration::from_secs(1);
                let base = event.clone();
                let mut progress = |p: f64| {
                    if last.elapsed() < Duration::from_millis(120) && p < 1.0 {
                        return;
                    }
                    last = Instant::now();
                    let mut update = base.clone();
                    update.progress = p;
                    if p > 0.01 && kind == JobKind::Compress {
                        update.remaining_seconds =
                            Some(start.elapsed().as_secs_f64() / p * (1.0 - p));
                    }
                    emit(update);
                };
                if kind == JobKind::Estimate {
                    let estimate = tools.estimate(
                        &input.path,
                        &input.media,
                        &settings,
                        &self.cancel,
                        &mut progress,
                    )?;
                    let meta = fs::metadata(&input.path).map_err(|e| e.to_string())?;
                    if meta.len() != input.media.bytes || meta.modified().ok() != input.modified {
                        return Err(
                            "예측 중 원본이 변경되었습니다. 파일을 다시 추가해 주세요.".into()
                        );
                    }
                    event.estimated_bytes = Some(estimate);
                } else {
                    let out = if let Some(source) = &input.folder_source {
                        source.output_dir(folder.as_deref().map(Path::new))?
                    } else {
                        folder.as_ref().map(PathBuf::from).unwrap_or_else(|| {
                            input
                                .path
                                .parent()
                                .unwrap_or(Path::new("."))
                                .join("Compressed")
                        })
                    };
                    let result = tools.compress(
                        &input.path,
                        &input.media,
                        &settings,
                        &out,
                        &self.cancel,
                        &mut progress,
                    )?;
                    event.output_bytes = Some(result.output_bytes);
                    event.output_path = result.output_path.clone();
                    if !result.saved {
                        event.phase = "unchanged".into();
                        event.message = Some("원본 유지 · 압축 이득 없음".into());
                    }
                    if let Some(path) = result.output_path {
                        self.outputs
                            .lock()
                            .map_err(|_| "결과 목록을 잠그지 못했습니다.")?
                            .insert(input.item.id.clone(), PathBuf::from(path));
                    }
                }
                Ok::<_, String>(())
            })();
            match result {
                Ok(()) => {
                    if event.phase == "processing" {
                        event.phase = "done".into();
                    }
                    event.progress = 1.0;
                }
                Err(error) => {
                    event.phase = if self.cancel.load(Ordering::Acquire) {
                        "cancelled"
                    } else {
                        "error"
                    }
                    .into();
                    event.message = Some(error);
                }
            }
            emit(event);
        }
        Ok(())
    }
}
