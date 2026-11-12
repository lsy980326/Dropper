use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Savings {
    pub bytes: i128,
    pub percent: Option<f64>,
}

pub fn savings(original: u64, output: u64) -> Savings {
    let bytes = i128::from(original) - i128::from(output);
    Savings {
        bytes,
        percent: (original > 0).then(|| bytes as f64 / original as f64 * 100.0),
    }
}

pub fn sample_windows(duration: f64) -> Vec<(f64, f64)> {
    if !duration.is_finite() || duration <= 0.0 {
        return vec![];
    }
    if duration <= 12.0 {
        return vec![(0.0, duration)];
    }
    [0.1, 0.5, 0.9]
        .into_iter()
        .map(|fraction| ((duration * fraction - 2.0).clamp(0.0, duration - 4.0), 4.0))
        .collect()
}

pub fn estimate_size(samples: &[(f64, u64)], duration: f64, audio_bps: u64) -> Option<u64> {
    if !duration.is_finite()
        || duration <= 0.0
        || samples.is_empty()
        || samples
            .iter()
            .any(|(d, bytes)| !d.is_finite() || *d <= 0.0 || *bytes == 0)
    {
        return None;
    }
    let seconds: f64 = samples.iter().map(|(d, _)| d).sum();
    let bytes: f64 = samples.iter().map(|(_, b)| *b as f64).sum();
    // Small independently encoded samples have extra GOP/container overhead.
    // This is an estimate, never a confidence interval or a promised saving.
    let total = (bytes / seconds + audio_bps as f64 / 8.0) * duration * 1.02;
    (total.is_finite() && total > 0.0 && total < u64::MAX as f64).then(|| total.round() as u64)
}
