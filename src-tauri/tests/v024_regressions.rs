use dropper_lib::{
    acceleration::{Backend, Capabilities},
    command::{encode_args, output_extension, Codec, Effort, Goal, Settings},
    engine::Tools,
    folder,
    media::Media,
    recommendation::{candidates, choose, recommend, Measurement},
};
use serde_json::json;
use std::{fs, path::Path, sync::atomic::AtomicBool};
fn source() -> Media {
    Media::from_probe(json!({"streams":[{"index":0,"codec_type":"video","codec_name":"h264","pix_fmt":"yuv420p","width":640,"height":360},{"index":1,"codec_type":"audio","codec_name":"aac"},{"index":2,"codec_type":"data","codec_name":"bin_data","codec_tag_string":"gpmd"}],"format":{"duration":"5"}}),10000).unwrap()
}
#[test]
fn auxiliary_tracks_are_described_and_omitted_only_when_requested() {
    let media = source();
    assert!(media.blocked.is_none());
    assert_eq!(media.auxiliary_tracks().len(), 1);
    assert!(output_extension(&media, &Settings::default()).is_err());
    let args = encode_args(
        Path::new("in.mp4"),
        Path::new("out.mp4"),
        &media,
        &Settings {
            strip_data: true,
            ..Settings::default()
        },
        None,
    )
    .unwrap();
    assert!(args.windows(2).any(|p| p == ["-map", "-0:2"]));
    assert!(args.windows(2).any(|p| p == ["-write_tmcd", "0"]));
    assert!(args.windows(2).any(|p| p == ["-c:a", "copy"]));
}
#[test]
fn automatic_size_mode_avoids_extreme_cpu_effort_and_discards_very_slow_candidates() {
    let mut media = source();
    media.streams.retain(|s| s.codec_type != "data");
    let caps = Capabilities {
        codecs: vec![Codec::H264, Codec::H265],
        hardware: vec![],
        cpu_threads: 4,
    };
    let settings = Settings {
        goal: Goal::Size,
        recommendation: true,
        ..Settings::default()
    };
    let result = recommend(
        &Tools::system().unwrap(),
        Path::new("unused"),
        &media,
        &settings,
        &caps,
        4,
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(result.choice.settings.effort, Effort::Balanced);
    let choices = candidates(&caps, &media, &settings, 4);
    let results = [
        Measurement {
            choice: choices[0].clone(),
            bytes_per_frame: 100.,
            seconds_per_frame: 0.01,
            similarity: 0.97,
        },
        Measurement {
            choice: choices[1].clone(),
            bytes_per_frame: 95.,
            seconds_per_frame: 0.04,
            similarity: 0.97,
        },
        Measurement {
            choice: choices[1].clone(),
            bytes_per_frame: 70.,
            seconds_per_frame: 0.02,
            similarity: 0.97,
        },
    ];
    assert_eq!(choose(&results, Goal::Size), Some(2));
    assert_eq!(choose(&results[..2], Goal::Size), Some(0));
    let similar_size = [
        Measurement {
            bytes_per_frame: 100.,
            seconds_per_frame: 0.01,
            ..results[0].clone()
        },
        Measurement {
            bytes_per_frame: 96.,
            seconds_per_frame: 0.025,
            ..results[0].clone()
        },
    ];
    assert_eq!(choose(&similar_size, Goal::Size), Some(0));
    assert_eq!(result.choice.profile.backend, Backend::Cpu);
}
#[test]
fn folder_names_do_not_hide_originals_and_only_marked_outputs_are_skipped() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    for name in [
        "a.mp4",
        "Compressed/original.avi",
        "trip/Compressed/clip.MOV",
        "extra/file.3g2",
        "extra/camera.MXF",
        "generated/result.mp4",
    ] {
        let p = root.join(name);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, b"fixture").unwrap();
    }
    folder::mark_output(&root.join("generated")).unwrap();
    let scan = folder::scan(
        &root,
        Some(&root.join("Compressed")),
        &AtomicBool::new(false),
        &mut |_| {},
    )
    .unwrap();
    assert_eq!(scan.files.len(), 5);
    assert_eq!(scan.skipped_results, 1);
}
