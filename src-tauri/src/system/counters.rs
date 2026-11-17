//! Rates from monotonically increasing per-device counters.
//!
//! Rates are computed per device and summed, using only devices present in
//! both samples. Summing totals first is wrong: when a volume or interface
//! appears between two samples, its lifetime counter shows up as a huge
//! one-tick spike (e.g. "123 GB/s").

use std::collections::HashMap;
use std::time::Instant;

pub type Counters = HashMap<String, (u64, u64)>;

/// Sum of per-key deltas for keys present in both maps. A counter that went
/// backwards (device reset / re-attached) contributes nothing.
pub fn delta(prev: &Counters, cur: &Counters) -> (u64, u64) {
    let mut a = 0u64;
    let mut b = 0u64;
    for (k, (ca, cb)) in cur {
        if let Some((pa, pb)) = prev.get(k) {
            if ca >= pa && cb >= pb {
                a += ca - pa;
                b += cb - pb;
            }
        }
    }
    (a, b)
}

#[derive(Default)]
pub struct CounterRates {
    prev: Option<(Instant, Counters)>,
}

impl CounterRates {
    /// Returns (rate_a, rate_b) per second, or `None` on the first sample.
    pub fn update(&mut self, now: Instant, cur: Counters) -> Option<(f64, f64)> {
        let out = self.prev.as_ref().and_then(|(t, p)| {
            let dt = now.duration_since(*t).as_secs_f64();
            (dt > 0.05).then(|| {
                let (a, b) = delta(p, &cur);
                (a as f64 / dt, b as f64 / dt)
            })
        });
        self.prev = Some((now, cur));
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(v: &[(&str, u64, u64)]) -> Counters {
        v.iter().map(|(k, a, b)| (k.to_string(), (*a, *b))).collect()
    }

    #[test]
    fn new_device_does_not_spike() {
        let p = m(&[("disk0", 1_000, 500)]);
        let c = m(&[("disk0", 1_100, 520), ("disk4", 123_000_000_000, 44_000_000_000)]);
        assert_eq!(delta(&p, &c), (100, 20));
    }

    #[test]
    fn removed_or_reset_device_is_ignored() {
        let p = m(&[("en0", 5_000, 5_000), ("en5", 900, 900)]);
        let c = m(&[("en0", 5_300, 5_100), ("en5", 10, 10)]);
        assert_eq!(delta(&p, &c), (300, 100));
    }

    #[test]
    fn first_sample_has_no_rate() {
        let mut r = CounterRates::default();
        let t0 = Instant::now();
        assert!(r.update(t0, m(&[("a", 0, 0)])).is_none());
        let (a, _) = r.update(t0 + std::time::Duration::from_secs(2), m(&[("a", 2000, 0)])).unwrap();
        assert!((a - 1000.0).abs() < 1.0);
    }
}
