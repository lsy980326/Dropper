use dropper_lib::process::ProcessError;

#[test]
fn only_pre_frame_hardware_initialization_errors_allow_a_cpu_retry() {
    let startup = ProcessError::Exit {
        details: "[hevc_nvenc] OpenEncodeSessionEx failed: out of memory (10)".into(),
        frames_encoded: false,
    };
    assert!(startup.hardware_initialization_failed());
    let encoding = ProcessError::Exit {
        details: "[hevc_nvenc] OpenEncodeSessionEx failed".into(),
        frames_encoded: true,
    };
    assert!(!encoding.hardware_initialization_failed());
    for details in [
        "Error opening output: No space left on device",
        "Error writing trailer: Permission denied",
        "Invalid data found when processing input",
        "EncodePicture failed: device lost",
    ] {
        assert!(!ProcessError::Exit {
            details: details.into(),
            frames_encoded: false
        }
        .hardware_initialization_failed());
    }
    assert!(
        !ProcessError::Message("작업이 취소되었습니다.".into()).hardware_initialization_failed()
    );
    assert!(!ProcessError::Exit {
        details: "Error opening input file C:/no capable devices found.mp4: Permission denied"
            .into(),
        frames_encoded: false
    }
    .hardware_initialization_failed());
}

#[test]
fn hardware_retry_is_once_and_never_masks_cancellation_or_disk_failure() {
    use dropper_lib::acceleration::{run_with_cpu_fallback, Backend, EncodingProfile};
    use std::sync::atomic::AtomicBool;
    let gpu = EncodingProfile {
        backend: Backend::Nvidia,
        threads: 8,
    };
    let mut attempts = vec![];
    let value = run_with_cpu_fallback(gpu, &AtomicBool::new(false), |profile, retry| {
        attempts.push((profile.backend, retry));
        if !retry {
            Err(ProcessError::Exit {
                details: "[hevc_nvenc @ 1234] OpenEncodeSessionEx failed".into(),
                frames_encoded: false,
            })
        } else {
            Ok(42)
        }
    })
    .unwrap();
    assert_eq!(value, 42);
    assert_eq!(attempts, [(Backend::Nvidia, false), (Backend::Cpu, true)]);
    for (cancelled, error) in [
        (true, "[hevc_nvenc] OpenEncodeSessionEx failed"),
        (false, "No space left on device"),
    ] {
        let mut count = 0;
        let result = run_with_cpu_fallback::<()>(gpu, &AtomicBool::new(cancelled), |_, _| {
            count += 1;
            Err(ProcessError::Exit {
                details: error.into(),
                frames_encoded: false,
            })
        });
        assert!(result.is_err());
        assert_eq!(count, 1);
    }
}

#[test]
fn process_progress_prevents_retry_after_frames_have_been_encoded() {
    use dropper_lib::process::execute_checked;
    use std::{ffi::OsString, path::Path, sync::atomic::AtomicBool, time::Duration};
    let python = if cfg!(windows) { "python" } else { "python3" };
    let args: Vec<OsString> = ["-c", "import sys; sys.stdout.buffer.write(b'frame=1\\r\\nout_time_us=1000000\\r\\n'); sys.stdout.flush(); print('[hevc_nvenc] OpenEncodeSessionEx failed', file=sys.stderr); sys.exit(1)"].into_iter().map(Into::into).collect();
    let mut progress = vec![];
    let error = execute_checked(
        Path::new(python),
        &args,
        &AtomicBool::new(false),
        Duration::from_secs(5),
        false,
        &mut |p| progress.push(p),
    )
    .unwrap_err();
    assert!(matches!(
        error,
        ProcessError::Exit {
            frames_encoded: true,
            ..
        }
    ));
    assert!(!error.hardware_initialization_failed());
    assert_eq!(progress, [1.0]);
}

#[test]
fn final_progress_is_delivered_even_when_reader_finishes_after_failed_child() {
    use dropper_lib::process::execute_checked;
    use std::{ffi::OsString, path::Path, sync::atomic::AtomicBool, time::Duration};
    let python = if cfg!(windows) { "python" } else { "python3" };
    let script = r#"import subprocess,sys; subprocess.Popen([sys.executable,'-c',"import sys,time; time.sleep(0.2); sys.stdout.buffer.write(b'frame=1\\r\\nout_time_us=1000000\\r\\n'); sys.stdout.flush()"],stdout=sys.stdout,stderr=sys.stderr); print('[hevc_nvenc] OpenEncodeSessionEx failed',file=sys.stderr); sys.exit(1)"#;
    let args: Vec<OsString> = vec!["-c".into(), script.into()];
    let mut progress = vec![];
    let error = execute_checked(
        Path::new(python),
        &args,
        &AtomicBool::new(false),
        Duration::from_secs(5),
        false,
        &mut |p| progress.push(p),
    )
    .unwrap_err();
    assert!(!error.hardware_initialization_failed());
    assert_eq!(progress, [1.0]);
}

#[test]
fn known_ffmpeg_backend_initialization_messages_allow_safe_cpu_recovery() {
    for details in [
        "[hevc_qsv @ 1234] Error initializing the encoder: invalid video parameters",
        "[hevc_amf @ 1234] encoder->Init() failed with error 1",
        "[hevc_nvenc @ 1234] No CUDA capable devices found",
        "[hevc_videotoolbox @ 1234] Cannot prepare encoder",
    ] {
        assert!(
            ProcessError::Exit {
                details: details.into(),
                frames_encoded: false
            }
            .hardware_initialization_failed(),
            "{details}"
        );
    }
}
