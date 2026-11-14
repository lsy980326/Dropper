use crate::{
    acceleration::{
        cpu_threads, run_with_cpu_fallback, Backend, Capabilities, HardwareEncoder, WorkBudget,
    },
    archive::{self, ArchivePlan, ResultRecord, SourceSnapshot},
    command::{output_extension, Codec, Settings},
    engine::{canonical_input, CompressionResult, Tools},
    folder::{self, FolderSource},
    media::Media,
    process::ProcessError,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex, OnceLock,
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
    pub data_tracks: Vec<String>,
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
    archived: bool,
    original_path: PathBuf,
}

#[derive(Clone)]
struct RejectedInput {
    path: PathBuf,
    folder_source: Option<FolderSource>,
    item: FileItem,
}

struct BatchContext<'a> {
    settings: &'a Settings,
    folder: Option<&'a str>,
    kind: JobKind,
    run_id: u64,
    tools: &'a Tools,
    capabilities: &'a Capabilities,
    threads: usize,
}

enum JobResult {
    Estimate(u64),
    Compressed(CompressionResult),
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backend: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backend_note: Option<String>,
    pub archived_path: Option<String>,
    pub original_moved: bool,
    pub archive_warning: Option<String>,
    pub report_path: Option<String>,
    pub stage: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Environment {
    pub ready: bool,
    pub source: String,
    pub codecs: Vec<Codec>,
    pub error: Option<String>,
    pub cpu_threads: usize,
    pub gpu_encoders: Vec<HardwareEncoder>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportResult {
    pub files: Vec<FileItem>,
    pub warnings: Vec<String>,
    pub skipped_results: usize,
    pub skipped_links: usize,
    pub ignored_files: usize,
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
    rejected: Mutex<HashMap<String, RejectedInput>>,
    outputs: Mutex<HashMap<String, PathBuf>>,
    reports: Mutex<HashMap<String, PathBuf>>,
    pub busy: AtomicBool,
    pub cancel: AtomicBool,
    next_id: AtomicU64,
    capabilities: OnceLock<Result<Capabilities, String>>,
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
        let mut ignored_files = 0;
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
                        ignored_files += scan.ignored_files;
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
            ignored_files,
            cancelled: self.cancel.load(Ordering::Acquire),
        })
    }
    pub fn new(tools: Result<Tools, String>, source: &str) -> Arc<Self> {
        Arc::new(Self {
            tools,
            source: source.into(),
            inputs: Mutex::new(HashMap::new()),
            rejected: Mutex::new(HashMap::new()),
            outputs: Mutex::new(HashMap::new()),
            reports: Mutex::new(HashMap::new()),
            busy: AtomicBool::new(false),
            cancel: AtomicBool::new(false),
            next_id: AtomicU64::new(1),
            capabilities: OnceLock::new(),
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
        match self.capabilities() {
            Ok(caps) => Environment {
                ready: true,
                source: self.source.clone(),
                codecs: caps.codecs.clone(),
                error: None,
                cpu_threads: caps.cpu_threads,
                gpu_encoders: caps.hardware.clone(),
            },
            Err(error) => Environment {
                ready: false,
                source: self.source.clone(),
                codecs: vec![],
                error: Some(error),
                cpu_threads: cpu_threads(),
                gpu_encoders: vec![],
            },
        }
    }
    fn capabilities(&self) -> Result<&Capabilities, String> {
        self.capabilities
            .get_or_init(|| {
                self.tools
                    .as_ref()
                    .map_err(Clone::clone)
                    .and_then(Capabilities::detect)
            })
            .as_ref()
            .map_err(Clone::clone)
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
            let path = path.canonicalize().unwrap_or(path);
            let existing = self
                .inputs
                .lock()
                .map_err(|_| "파일 목록 잠금 실패")?
                .values()
                .find(|i| i.path == path)
                .map(|i| i.item.id.clone());
            let rejected = self
                .rejected
                .lock()
                .map_err(|_| "분석 결과 잠금 실패")?
                .values()
                .find(|i| i.path == path)
                .map(|i| i.item.id.clone());
            let id = existing
                .or(rejected)
                .unwrap_or_else(|| format!("f{version}"));
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
                        && !input.archived
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
                    data_tracks: media
                        .auxiliary_tracks()
                        .iter()
                        .map(|stream| {
                            format!(
                                "#{} {} ({}, {})",
                                stream.index,
                                stream.codec_type,
                                stream.codec_name,
                                stream.codec_tag_string
                            )
                        })
                        .collect(),
                    folder_name: source
                        .as_ref()
                        .and_then(|s| s.root.file_name())
                        .map(|s| s.to_string_lossy().into_owned()),
                    relative_path: source
                        .as_ref()
                        .map(|s| s.relative_path.to_string_lossy().into_owned()),
                };
                self.rejected
                    .lock()
                    .map_err(|_| "분석 결과 잠금 실패")?
                    .remove(&actual_id);
                self.inputs
                    .lock()
                    .map_err(|_| "파일 목록을 잠그지 못했습니다.")?
                    .insert(
                        actual_id,
                        Input {
                            original_path: previous
                                .as_ref()
                                .map(|i| i.original_path.clone())
                                .unwrap_or_else(|| path.clone()),
                            path,
                            modified: meta.modified().ok(),
                            item: item.clone(),
                            media,
                            folder_source: source,
                            archived: previous.as_ref().is_some_and(|i| i.archived),
                        },
                    );
                Ok::<_, String>(item)
            })();
            result.push(analysis.unwrap_or_else(|reason| {
                let item = FileItem {
                    id,
                    version,
                    name,
                    bytes: fs::metadata(&path).map(|m| m.len()).unwrap_or(0),
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
                    data_tracks: vec![],
                    folder_name: folder_source
                        .as_ref()
                        .and_then(|s| s.root.file_name())
                        .map(|s| s.to_string_lossy().into_owned()),
                    relative_path: folder_source
                        .as_ref()
                        .map(|s| s.relative_path.to_string_lossy().into_owned()),
                };
                if let Ok(mut inputs) = self.inputs.lock() {
                    inputs.remove(&item.id);
                }
                if let Ok(mut rejected) = self.rejected.lock() {
                    rejected.insert(
                        item.id.clone(),
                        RejectedInput {
                            path: path.clone(),
                            folder_source: folder_source.clone(),
                            item: item.clone(),
                        },
                    );
                }
                item
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
        self.rejected
            .lock()
            .map_err(|_| "분석 결과 잠금 실패")?
            .remove(id);
        self.reports
            .lock()
            .map_err(|_| "결과 기록 잠금 실패")?
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
        self.rejected
            .lock()
            .map_err(|_| "분석 결과 잠금 실패")?
            .clear();
        self.reports
            .lock()
            .map_err(|_| "결과 기록 잠금 실패")?
            .clear();
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
    pub fn original_folder(&self, id: &str) -> Result<PathBuf, String> {
        self.inputs
            .lock()
            .map_err(|_| "파일 목록 잠금 실패")?
            .get(id)
            .and_then(|input| input.path.parent())
            .map(Path::to_path_buf)
            .ok_or_else(|| "원본 위치가 없습니다.".into())
    }
    pub fn report_folder(&self, id: &str) -> Result<PathBuf, String> {
        self.reports
            .lock()
            .map_err(|_| "결과 기록 잠금 실패")?
            .get(id)
            .and_then(|path| path.parent())
            .map(Path::to_path_buf)
            .ok_or_else(|| "작업 기록이 없습니다.".into())
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
        let rejected_inputs = self.rejected.lock().map_err(|_| "분석 결과 잠금 실패")?;
        let mut seen = HashSet::new();
        let mut selected = Vec::new();
        let mut rejected = Vec::new();
        for id in ids.into_iter().filter(|id| seen.insert(id.clone())) {
            if let Some(input) = inputs.get(&id) {
                selected.push(input.clone());
            } else if let Some(input) = rejected_inputs.get(&id) {
                rejected.push(input.clone());
            } else {
                return Err("목록에 등록되지 않은 영상입니다. 다시 추가해 주세요.".into());
            }
        }
        drop(rejected_inputs);
        drop(inputs);
        let budget = WorkBudget::new(cpu_threads(), selected.len(), &settings)?;
        let context = BatchContext {
            settings: &settings,
            folder: folder.as_deref(),
            kind,
            run_id,
            tools,
            capabilities: self.capabilities()?,
            threads: budget.threads_per_worker,
        };
        for rejected in rejected {
            let mut event = JobEvent {
                run_id,
                id: rejected.item.id.clone(),
                kind,
                phase: "error".into(),
                progress: 1.0,
                estimated_bytes: None,
                output_bytes: None,
                output_path: None,
                message: rejected.item.blocked.clone(),
                remaining_seconds: None,
                backend: None,
                backend_note: None,
                archived_path: None,
                original_moved: false,
                archive_warning: None,
                report_path: None,
                stage: None,
            };
            if (settings.archive_originals || settings.organize_output) && kind == JobKind::Compress
            {
                match ArchivePlan::new(
                    &rejected.path,
                    rejected.folder_source.as_ref(),
                    folder.as_deref().map(Path::new),
                ) {
                    Ok(plan) => {
                        match archive::record_result(
                            &plan,
                            &ResultRecord {
                                id: &rejected.item.id,
                                name: &rejected.item.name,
                                status: "분석 실패",
                                source: &rejected.path,
                                output: None,
                                archive: None,
                                message: rejected.item.blocked.as_deref(),
                            },
                        ) {
                            Ok(path) => {
                                event.report_path = Some(path.to_string_lossy().into_owned());
                                self.reports
                                    .lock()
                                    .map_err(|_| "기록 잠금 실패")?
                                    .insert(rejected.item.id, path);
                            }
                            Err(error) => {
                                event.archive_warning =
                                    Some(format!("작업 기록 저장 실패: {error}"))
                            }
                        }
                    }
                    Err(error) => {
                        event.archive_warning = Some(format!("작업 기록 저장 실패: {error}"))
                    }
                }
            }
            emit(event);
        }
        crate::parallel::run(
            selected,
            budget.workers,
            |input, send| self.process_one(input, &context, send),
            emit,
        );
        Ok(())
    }

    fn process_one(
        &self,
        input: Input,
        context: &BatchContext<'_>,
        emit: &mut dyn FnMut(JobEvent),
    ) {
        let BatchContext {
            settings,
            folder,
            kind,
            run_id,
            tools,
            ..
        } = *context;
        let profile = context
            .capabilities
            .profile(&input.media, settings, context.threads);
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
            backend: Some(profile.backend.label().into()),
            archived_path: None,
            original_moved: false,
            archive_warning: None,
            report_path: None,
            stage: Some("encoding".into()),
            backend_note: context
                .capabilities
                .cpu_reason(&input.media, settings)
                .map(str::to_string),
        };
        let plan = if kind == JobKind::Compress
            && (settings.archive_originals || settings.organize_output)
        {
            Some(ArchivePlan::new(
                &input.original_path,
                input.folder_source.as_ref(),
                folder.map(Path::new),
            ))
        } else {
            None
        };
        if self.cancel.load(Ordering::Acquire) {
            event.phase = "cancelled".into();
            event.message = Some("취소됨".into());
            event.backend = None;
            self.record_event(
                &input,
                plan.as_ref().and_then(|p| p.as_ref().ok()),
                &mut event,
            );
            emit(event);
            return;
        }
        emit(event.clone());
        let result = (|| {
            if let Some(reason) = &input.media.blocked {
                return Err(reason.clone().into());
            }
            let mut snapshot =
                if settings.archive_originals && kind == JobKind::Compress && !input.archived {
                    Some(SourceSnapshot::capture(&input.path)?)
                } else {
                    None
                };
            let meta = fs::metadata(&input.path)
                .map_err(|_| "원본을 찾을 수 없습니다. 파일을 다시 추가해 주세요.")?;
            if meta.len() != input.media.bytes || meta.modified().ok() != input.modified {
                return Err("추가한 뒤 원본이 변경되었습니다. 파일을 다시 추가해 주세요.".into());
            }
            let recommended = if settings.recommendation {
                event.stage = Some("recommendation".into());
                event.backend_note = Some("현재 PC에 맞는 설정을 짧게 시험하고 있어요.".into());
                emit(event.clone());
                Some(crate::recommendation::recommend(
                    tools,
                    &input.path,
                    &input.media,
                    settings,
                    context.capabilities,
                    context.threads,
                    &self.cancel,
                )?)
            } else {
                None
            };
            let settings = recommended
                .as_ref()
                .map(|r| &r.choice.settings)
                .unwrap_or(settings);
            let profile = recommended
                .as_ref()
                .map(|r| r.choice.profile)
                .unwrap_or(profile);
            if let Some(ref recommendation) = recommended {
                event.backend = Some(profile.backend.label().into());
                event.backend_note = Some(recommendation.note.clone());
                emit(event.clone());
            }
            let meta = fs::metadata(&input.path).map_err(|e| e.to_string())?;
            if meta.len() != input.media.bytes || meta.modified().ok() != input.modified {
                return Err("시험 중 원본이 변경되었습니다. 파일을 다시 추가해 주세요.".into());
            }
            output_extension(&input.media, settings)?;
            let out = if kind == JobKind::Compress {
                Some(if let Some(plan) = &plan {
                    plan.as_ref().map_err(Clone::clone)?.results_dir()?
                } else if let Some(source) = &input.folder_source {
                    source.output_dir(folder.map(Path::new))?
                } else {
                    folder.map(PathBuf::from).unwrap_or_else(|| {
                        input
                            .path
                            .parent()
                            .unwrap_or(Path::new("."))
                            .join("Compressed")
                    })
                })
            } else {
                None
            };
            event.stage = Some("encoding".into());
            let result = run_with_cpu_fallback(profile, &self.cancel, |current, retried| {
                if retried {
                    event.backend = Some(Backend::Cpu.label().into());
                    event.backend_note = Some("GPU 가속을 시작하지 못해 CPU로 전환했습니다. 그래픽 드라이버를 확인해 주세요.".into());
                    emit(event.clone());
                }
                let start = Instant::now();
                let mut last = Instant::now() - Duration::from_secs(1);
                let base = event.clone();
                let mut progress = |p: f64| {
                    if last.elapsed() < Duration::from_millis(120) && p < 1.0 {
                        return;
                    }
                    last = Instant::now();
                    let mut update = base.clone();
                    update.progress = if kind == JobKind::Compress && settings.archive_originals {
                        p.min(0.98)
                    } else {
                        p
                    };
                    if p > 0.01 && kind == JobKind::Compress {
                        update.remaining_seconds =
                            Some(start.elapsed().as_secs_f64() / p * (1.0 - p));
                    }
                    emit(update);
                };
                if kind == JobKind::Estimate {
                    let estimate = tools.estimate_with_profile(
                        &input.path,
                        &input.media,
                        settings,
                        &self.cancel,
                        &mut progress,
                        current,
                    )?;
                    let meta = fs::metadata(&input.path).map_err(|e| e.to_string())?;
                    if meta.len() != input.media.bytes || meta.modified().ok() != input.modified {
                        return Err(
                            "예측 중 원본이 변경되었습니다. 파일을 다시 추가해 주세요.".into()
                        );
                    }
                    Ok(JobResult::Estimate(estimate))
                } else {
                    tools
                        .compress_with_profile(
                            &input.path,
                            &input.media,
                            settings,
                            out.as_deref().expect("Compression output directory"),
                            &self.cancel,
                            &mut progress,
                            current,
                        )
                        .map(JobResult::Compressed)
                }
            })?;
            match result {
                JobResult::Estimate(estimate) => event.estimated_bytes = Some(estimate),
                JobResult::Compressed(result) => {
                    event.output_bytes = Some(result.output_bytes);
                    event.output_path = result.output_path.clone();
                    if !result.saved {
                        event.phase = "unchanged".into();
                        event.message = Some("원본 유지 · 압축 이득 없음".into());
                    }
                    if result.saved {
                        if let (Some(snapshot), Some(Ok(plan)), Some(path)) = (
                            snapshot.as_mut(),
                            plan.as_ref(),
                            result.output_path.as_deref(),
                        ) {
                            event.stage = Some("archiving".into());
                            event.progress = 0.99;
                            event.remaining_seconds = None;
                            emit(event.clone());
                            let outcome = archive::move_original(
                                snapshot,
                                plan,
                                Path::new(path),
                                &self.cancel,
                            );
                            event.original_moved = outcome.moved;
                            event.archived_path = outcome
                                .path
                                .as_ref()
                                .map(|p| p.to_string_lossy().into_owned());
                            event.archive_warning = outcome.warning;
                            if outcome.moved {
                                if let Some(path) = outcome.path {
                                    match self.inputs.lock() {
                                        Ok(mut inputs)=>if let Some(current)=inputs.get_mut(&input.item.id){current.modified=fs::metadata(&path).ok().and_then(|m|m.modified().ok());current.path=path;current.archived=true;},
                                        Err(_)=>event.archive_warning=Some("압축·보관 완료, 앱 목록 갱신 실패. 기록된 보관 위치를 확인해 주세요.".into()),
                                    }
                                }
                            }
                        } else if input.archived {
                            event.original_moved = true;
                            event.archived_path = Some(input.path.to_string_lossy().into_owned());
                        }
                    }
                    if let Some(path) = result.output_path {
                        match self.outputs.lock() {
                            Ok(mut outputs) => {
                                outputs.insert(input.item.id.clone(), PathBuf::from(path));
                            }
                            Err(_) => event.archive_warning = Some(
                                "압축 완료, 결과 목록 갱신 실패. 결과 파일 위치를 확인해 주세요."
                                    .into(),
                            ),
                        }
                    }
                }
            }
            Ok::<_, ProcessError>(())
        })();
        match result {
            Ok(()) => {
                if event.phase == "processing" {
                    event.phase = "done".into();
                }
                event.progress = 1.0;
                event.stage = None;
            }
            Err(error) => {
                event.phase = if self.cancel.load(Ordering::Acquire) {
                    "cancelled"
                } else {
                    "error"
                }
                .into();
                event.message = Some(if self.cancel.load(Ordering::Acquire) {
                    "취소됨".into()
                } else {
                    error.to_string()
                });
            }
        }
        self.record_event(
            &input,
            plan.as_ref().and_then(|p| p.as_ref().ok()),
            &mut event,
        );
        emit(event);
    }
    fn record_event(&self, input: &Input, plan: Option<&ArchivePlan>, event: &mut JobEvent) {
        if let Some(plan) = plan {
            let status = match event.phase.as_str() {
                "done" if event.archive_warning.is_some() => "압축 성공 · 보관/기록 확인 필요",
                "done" => "성공",
                "unchanged" => "원본 유지 · 압축 이득 없음",
                "cancelled" => "취소",
                _ => "실패",
            };
            let message = event
                .archive_warning
                .as_deref()
                .or(event.message.as_deref());
            match archive::record_result(
                plan,
                &ResultRecord {
                    id: &input.item.id,
                    name: &input.item.name,
                    status,
                    source: &input.original_path,
                    output: event.output_path.as_deref(),
                    archive: event.archived_path.as_deref(),
                    message,
                },
            ) {
                Ok(path) => {
                    event.report_path = Some(path.to_string_lossy().into_owned());
                    if let Ok(mut reports) = self.reports.lock() {
                        reports.insert(input.item.id.clone(), path);
                    }
                }
                Err(error) => {
                    let prefix = event
                        .archive_warning
                        .take()
                        .map(|s| s + " · ")
                        .unwrap_or_default();
                    event.archive_warning = Some(prefix + "작업 기록 저장 실패: " + &error);
                }
            }
        }
    }
}
