pub fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

pub const DAY_MS: i64 = 86_400_000;

/// Start of the UTC day containing `ts_ms`.
pub fn utc_day_start(ts_ms: i64) -> i64 {
    ts_ms.div_euclid(DAY_MS) * DAY_MS
}

pub fn utc_day_label(ts_ms: i64) -> String {
    chrono::DateTime::from_timestamp_millis(ts_ms)
        .map(|d| d.format("%Y-%m-%d").to_string())
        .unwrap_or_default()
}

pub fn utc_month_label(ts_ms: i64) -> String {
    chrono::DateTime::from_timestamp_millis(ts_ms)
        .map(|d| d.format("%Y-%m").to_string())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn day_math() {
        let ts = 1_700_000_123_456; // 2023-11-14T22:15:23Z
        assert_eq!(utc_day_label(utc_day_start(ts)), "2023-11-14");
        assert_eq!(utc_day_start(ts) % DAY_MS, 0);
        assert_eq!(utc_month_label(ts), "2023-11");
    }
}
