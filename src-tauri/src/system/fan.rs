use super::{FanMetrics, MetricStatus};

/// Fan speed requires reading AppleSMC keys (`FNum`, `F0Ac`) through a private
/// IOKit user client. That interface is undocumented, and this project's
/// reference hardware (MacBook Air M1) has no fan, so it could not be verified.
/// We therefore report honestly instead of guessing.
pub fn collect(fanless: Option<bool>) -> FanMetrics {
    match fanless {
        Some(true) => FanMetrics {
            status: MetricStatus::NotPresent,
            note: Some("This Mac model is fanless (passively cooled).".into()),
            rpm: vec![],
        },
        _ => FanMetrics {
            status: MetricStatus::Unsupported,
            note: Some(
                "Fan RPM needs private AppleSMC access, which is not implemented. Use a dedicated SMC tool if required."
                    .into(),
            ),
            rpm: vec![],
        },
    }
}
