use dropper_lib::policy::{estimate_size, sample_windows, savings};

#[test]
fn reports_actual_reduction_and_growth_without_unsigned_wrap() {
    assert_eq!(savings(1_000, 400).bytes, 600);
    assert_eq!(savings(1_000, 400).percent, Some(60.0));
    assert_eq!(savings(400, 1_000).bytes, -600);
    assert_eq!(savings(400, 1_000).percent, Some(-150.0));
    assert_eq!(savings(0, 0).percent, None);
}

#[test]
fn samples_short_clips_once_and_long_clips_across_the_timeline() {
    assert_eq!(sample_windows(2.5), vec![(0.0, 2.5)]);
    let windows = sample_windows(120.0);
    assert_eq!(windows.len(), 3);
    assert!(windows.first().unwrap().0 < 25.0);
    assert!(windows.last().unwrap().0 > 90.0);
    for pair in windows.windows(2) {
        assert!(pair[0].0 + pair[0].1 <= pair[1].0);
    }
    assert!(windows.iter().all(|(s, d)| *d > 0.0 && s + d <= 120.0));
    assert!(sample_windows(f64::NAN).is_empty());
    assert!(sample_windows(0.0).is_empty());
}

#[test]
fn estimates_by_sample_duration_and_adds_audio_once() {
    // 300 KB / 6 s video, over 60 s = 3 MB; 128 kbps audio = 960 KB.
    assert_eq!(
        estimate_size(&[(2.0, 100_000), (4.0, 200_000)], 60.0, 128_000),
        Some(4_039_200)
    );
    assert_eq!(estimate_size(&[], 60.0, 0), None);
    assert_eq!(estimate_size(&[(0.0, 100)], 60.0, 0), None);
    assert_eq!(estimate_size(&[(1.0, 100)], f64::INFINITY, 0), None);
}
