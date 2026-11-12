use crate::{
    command::Settings,
    engine::Tools,
    jobs::{Environment, FileItem, ImportResult, JobKind, Runtime},
};
use std::{
    sync::{atomic::Ordering, Arc},
    time::Duration,
};
use tauri::{Emitter, Manager, State};

#[tauri::command]
async fn get_environment(state: State<'_, Arc<Runtime>>) -> Result<Environment, String> {
    let runtime = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || runtime.environment())
        .await
        .map_err(|e| e.to_string())
}
#[tauri::command]
async fn add_files(
    paths: Vec<String>,
    state: State<'_, Arc<Runtime>>,
) -> Result<Vec<FileItem>, String> {
    let runtime = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || runtime.add(paths))
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
async fn import_paths(
    paths: Vec<String>,
    output_folder: Option<String>,
    run_id: u64,
    app: tauri::AppHandle,
    state: State<'_, Arc<Runtime>>,
) -> Result<ImportResult, String> {
    let runtime = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        runtime.import_paths(paths, output_folder, run_id, &mut |progress| {
            let _ = app.emit("import-progress", progress);
        })
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
fn clear_files(state: State<'_, Arc<Runtime>>) -> Result<(), String> {
    state.clear()
}
#[tauri::command]
fn remove_file(id: String, state: State<'_, Arc<Runtime>>) -> Result<(), String> {
    state.remove(&id)
}
#[tauri::command]
async fn run_jobs(
    ids: Vec<String>,
    settings: Settings,
    folder: Option<String>,
    kind: JobKind,
    run_id: u64,
    app: tauri::AppHandle,
    state: State<'_, Arc<Runtime>>,
) -> Result<(), String> {
    let runtime = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        runtime.work(ids, settings, folder, kind, run_id, &mut |event| {
            let _ = app.emit("job-progress", event);
        })
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
fn cancel_active(state: State<'_, Arc<Runtime>>) {
    state.cancel.store(true, Ordering::Release);
}
#[tauri::command]
fn reveal_output(id: String, state: State<'_, Arc<Runtime>>) -> Result<(), String> {
    let path = state.output_folder(&id)?;
    #[cfg(target_os = "macos")]
    let mut command = std::process::Command::new("/usr/bin/open");
    #[cfg(target_os = "windows")]
    let mut command = std::process::Command::new("explorer.exe");
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let mut command = std::process::Command::new("xdg-open");
    command
        .arg(path)
        .spawn()
        .map_err(|e| format!("폴더를 열지 못했습니다: {e}"))?;
    Ok(())
}

pub fn run() {
    let app=tauri::Builder::default().plugin(tauri_plugin_dialog::init()).setup(|app|{
        let root=app.path().resource_dir()?.join("media").join("bin");
        let ffmpeg=root.join(format!("ffmpeg{}",std::env::consts::EXE_SUFFIX));
        let ffprobe=root.join(format!("ffprobe{}",std::env::consts::EXE_SUFFIX));
        let (tools,source)=if ffmpeg.is_file()&&ffprobe.is_file(){(Ok(Tools{ffmpeg,ffprobe}),"bundled")}else if cfg!(debug_assertions){(Tools::system(),"system")}else{(Err("영상 엔진이 앱에 포함되어 있지 않습니다. 완전한 설치 파일로 다시 설치해 주세요.".into()),"missing")};
        app.manage(Runtime::new(tools,source));
        Ok(())
    }).invoke_handler(tauri::generate_handler![get_environment,add_files,import_paths,clear_files,remove_file,run_jobs,cancel_active,reveal_output])
      .build(tauri::generate_context!()).expect("드롭퍼를 시작할 수 없습니다.");
    app.run(|app, event| {
        if let tauri::RunEvent::ExitRequested { api, .. } = event {
            let runtime = app.state::<Arc<Runtime>>().inner().clone();
            if runtime.busy.load(Ordering::Acquire) {
                api.prevent_exit();
                runtime.cancel.store(true, Ordering::Release);
                let handle = app.clone();
                std::thread::spawn(move || {
                    while runtime.busy.load(Ordering::Acquire) {
                        std::thread::sleep(Duration::from_millis(40));
                    }
                    handle.exit(0);
                });
            }
        }
    });
}
