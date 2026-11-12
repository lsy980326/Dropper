use dropper_lib::{
    command::{encode_args, output_extension, Codec, Container, Settings},
    media::Media,
};
use serde_json::{json, Value};
use std::path::Path;

fn probe(extra: Vec<Value>) -> Value {
    let mut streams = vec![
        json!({"index":0,"codec_type":"video","codec_name":"h264","width":1920,"height":1080,"pix_fmt":"yuv420p","field_order":"progressive"}),
    ];
    streams.extend(extra);
    json!({"streams":streams,"format":{"duration":"60.0"}})
}

#[test]
fn retains_subtitles_and_all_audio_by_switching_container() {
    let media = Media::from_probe(
        probe(vec![
            json!({"index":1,"codec_type":"audio","codec_name":"aac","bit_rate":"128000"}),
            json!({"index":2,"codec_type":"audio","codec_name":"aac","bit_rate":"192000"}),
            json!({"index":3,"codec_type":"subtitle","codec_name":"ass"}),
        ]),
        1_000_000,
    )
    .unwrap();
    assert_eq!(media.audio_bps(), Some(320_000));
    assert_eq!(
        output_extension(&media, &Settings::default()).unwrap(),
        "mkv"
    );
    let forced = Settings {
        container: Container::Mp4,
        ..Settings::default()
    };
    assert!(output_extension(&media, &forced)
        .unwrap_err()
        .contains("MKV"));
    let args = encode_args(
        Path::new("/tmp/한글 $(touch nope).mkv"),
        Path::new("/tmp/output.mkv"),
        &media,
        &Settings::default(),
        None,
    )
    .unwrap();
    assert!(args.windows(2).any(|p| p == ["-map", "0"]));
    assert!(args.windows(2).any(|p| p == ["-c:a", "copy"]));
    assert!(args.iter().any(|a| a == "/tmp/한글 $(touch nope).mkv"));
}

#[test]
fn hdr_keeps_ten_bit_and_refuses_unverified_codec_paths() {
    let mut value = probe(vec![]);
    value["streams"][0]["color_transfer"] = json!("smpte2084");
    value["streams"][0]["color_primaries"] = json!("bt2020");
    value["streams"][0]["color_space"] = json!("bt2020nc");
    value["streams"][0]["pix_fmt"] = json!("yuv420p10le");
    let media = Media::from_probe(value, 1_000_000).unwrap();
    assert_eq!(media.hdr.as_deref(), Some("HDR10"));
    let args = encode_args(
        Path::new("in.mov"),
        Path::new("out.mp4"),
        &media,
        &Settings::default(),
        None,
    )
    .unwrap();
    assert!(args.iter().any(|a| a == "yuv420p10le"));
    assert!(args.iter().any(|a| a == "smpte2084"));
    assert!(encode_args(
        Path::new("in.mov"),
        Path::new("out.mp4"),
        &media,
        &Settings {
            codec: Codec::H264,
            ..Settings::default()
        },
        None
    )
    .is_err());
}

#[test]
fn blocks_dynamic_hdr_instead_of_silently_flattening_it() {
    let mut value = probe(vec![]);
    value["streams"][0]["side_data_list"] = json!([{"side_data_type":"DOVI configuration record"}]);
    let media = Media::from_probe(value, 1_000_000).unwrap();
    assert!(media.blocked.unwrap().contains("Dolby"));
}

#[test]
fn rejects_nonvideo_and_unknown_duration_without_nan_estimates() {
    assert!(Media::from_probe(json!({"streams":[],"format":{"duration":"NaN"}}), 100).is_err());
    let mut value = probe(vec![]);
    value["format"]["duration"] = json!("N/A");
    assert!(Media::from_probe(value, 100).is_err());
}

#[test]
fn video_only_samples_do_not_duplicate_audio_or_subtitles() {
    let media = Media::from_probe(
        probe(vec![
            json!({"index":1,"codec_type":"audio","codec_name":"aac"}),
        ]),
        1000,
    )
    .unwrap();
    let args = encode_args(
        Path::new("in.mp4"),
        Path::new("out.mkv"),
        &media,
        &Settings::default(),
        Some((15.0, 4.0)),
    )
    .unwrap();
    assert!(args.iter().any(|a| a == "-an"));
    assert!(args.iter().any(|a| a == "-sn"));
    assert!(args.windows(2).any(|p| p == ["-t", "4.000000"]));
    assert!(args
        .windows(2)
        .any(|p| p == ["-fps_mode:v:0", "passthrough"]));
}

#[test]
fn mp4_subtitles_are_not_silently_copied_into_incompatible_matroska() {
    let media = Media::from_probe(
        probe(vec![
            json!({"index":1,"codec_type":"subtitle","codec_name":"mov_text"}),
        ]),
        10000,
    )
    .unwrap();
    assert_eq!(
        output_extension(&media, &Settings::default()).unwrap(),
        "mp4"
    );
    assert!(output_extension(
        &media,
        &Settings {
            container: Container::Mkv,
            ..Settings::default()
        }
    )
    .is_err());
}
