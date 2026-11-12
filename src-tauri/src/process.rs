use std::{
    collections::VecDeque,
    ffi::OsString,
    io::Read,
    path::Path,
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

pub fn execute(
    program: &Path,
    args: &[OsString],
    cancel: &AtomicBool,
    timeout: Duration,
    capture: bool,
    progress: &mut dyn FnMut(f64),
) -> Result<Vec<u8>, String> {
    if cancel.load(Ordering::Acquire) {
        return Err("작업이 취소되었습니다.".into());
    }
    let mut command = Command::new(program);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let mut child = command
        .spawn()
        .map_err(|e| format!("영상 엔진을 실행하지 못했습니다: {e}"))?;
    let mut stdout = child.stdout.take().ok_or("영상 엔진 출력 연결 실패")?;
    let mut stderr = child.stderr.take().ok_or("영상 엔진 오류 출력 연결 실패")?;
    let (tx, rx) = mpsc::channel();
    let output_thread = thread::spawn(move || -> Result<Vec<u8>, String> {
        if capture {
            let mut output = vec![];
            (&mut stdout)
                .take(8 * 1024 * 1024 + 1)
                .read_to_end(&mut output)
                .map_err(|e| e.to_string())?;
            if output.len() > 8 * 1024 * 1024 {
                return Err("영상 정보가 너무 커서 안전하게 읽을 수 없습니다.".into());
            }
            return Ok(output);
        }
        let mut buffer = [0_u8; 8192];
        let mut line = Vec::new();
        loop {
            let count = stdout.read(&mut buffer).map_err(|e| e.to_string())?;
            if count == 0 {
                break;
            }
            for byte in &buffer[..count] {
                if *byte == b'\n' {
                    if let Some(value) = String::from_utf8_lossy(&line).strip_prefix("out_time_us=")
                    {
                        if let Ok(us) = value.trim().parse::<f64>() {
                            let _ = tx.send(us / 1_000_000.0);
                        }
                    }
                    line.clear();
                } else if line.len() < 8192 {
                    line.push(*byte);
                }
            }
        }
        Ok(vec![])
    });
    let error_thread = thread::spawn(move || -> Result<Vec<u8>, String> {
        let mut tail = VecDeque::with_capacity(16384);
        let mut buffer = [0_u8; 4096];
        loop {
            let count = stderr.read(&mut buffer).map_err(|e| e.to_string())?;
            if count == 0 {
                break;
            }
            for b in &buffer[..count] {
                if tail.len() == 16384 {
                    tail.pop_front();
                }
                tail.push_back(*b);
            }
        }
        Ok(tail.into_iter().collect())
    });
    let start = Instant::now();
    let mut interrupted = None;
    let status = loop {
        for value in rx.try_iter() {
            if value.is_finite() && value >= 0.0 {
                progress(value);
            }
        }
        if cancel.load(Ordering::Acquire) || start.elapsed() >= timeout {
            interrupted = Some(if cancel.load(Ordering::Acquire) {
                "작업이 취소되었습니다."
            } else {
                "처리 제한 시간을 초과했습니다."
            });
            let _ = child.kill();
            break child.wait();
        }
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) => thread::sleep(Duration::from_millis(35)),
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                break Err(e);
            }
        }
    };
    let output = output_thread
        .join()
        .map_err(|_| "영상 엔진 출력 처리 실패")?;
    let errors = error_thread
        .join()
        .map_err(|_| "영상 엔진 오류 처리 실패")??;
    if let Some(message) = interrupted {
        return Err(message.into());
    }
    let status = status.map_err(|e| format!("영상 엔진 상태 확인 실패: {e}"))?;
    if !status.success() {
        let details = String::from_utf8_lossy(&errors).trim().to_string();
        return Err(if details.is_empty() {
            "영상 엔진이 정상적으로 완료되지 않았습니다.".into()
        } else {
            format!("영상을 처리할 수 없습니다: {details}")
        });
    }
    for value in rx.try_iter() {
        if value.is_finite() && value >= 0.0 {
            progress(value);
        }
    }
    output
}
