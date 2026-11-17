use super::counters::{CounterRates, Counters};
use super::{MetricStatus, NetworkMetrics};
use crate::util::now_ms;
use std::time::Instant;
use sysinfo::Networks;

/// Aggregate throughput over physical interfaces. Loopback, tunnels and
/// virtual bridges are excluded to avoid double counting VPN/VM traffic.
/// Rates are computed per interface (see `counters`), so an interface that
/// appears between samples cannot create a spike.
pub struct NetworkCollector {
    nets: Networks,
    rates: CounterRates,
}

impl Default for NetworkCollector {
    fn default() -> Self {
        Self {
            nets: Networks::new_with_refreshed_list().unwrap_or_else(|_| Networks::new().expect("networks")),
            rates: CounterRates::default(),
        }
    }
}

pub fn is_counted_interface(name: &str) -> bool {
    !(name.starts_with("lo")
        || name.starts_with("utun")
        || name.starts_with("awdl")
        || name.starts_with("llw")
        || name.starts_with("bridge")
        || name.starts_with("gif")
        || name.starts_with("stf")
        || name.starts_with("anpi")
        || name.starts_with("ap"))
}

impl NetworkCollector {
    pub fn collect(&mut self) -> NetworkMetrics {
        self.nets.refresh(true);
        let mut m = NetworkMetrics { updated_at: now_ms(), ..Default::default() };
        let (mut rx, mut tx) = (0u64, 0u64);
        let mut busiest: Option<(String, u64)> = None;
        let mut counters = Counters::new();
        for (name, data) in self.nets.iter() {
            let name = name.to_string_lossy().into_owned();
            if !is_counted_interface(&name) {
                continue;
            }
            let (r, t) = (data.total_received(), data.total_transmitted());
            rx += r;
            tx += t;
            if busiest.as_ref().is_none_or(|(_, b)| r + t > *b) {
                busiest = Some((name.clone(), r + t));
            }
            counters.insert(name, (r, t));
        }
        m.primary_interface = busiest.filter(|(_, t)| *t > 0).map(|(n, _)| n);
        m.total_rx = Some(rx);
        m.total_tx = Some(tx);
        if let Some((r, t)) = self.rates.update(Instant::now(), counters) {
            m.rx_bps = Some(r);
            m.tx_bps = Some(t);
        }
        m.status = if m.rx_bps.is_some() { MetricStatus::Ok } else { MetricStatus::Pending };
        m
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn filters_virtual_interfaces() {
        assert!(is_counted_interface("en0"));
        assert!(!is_counted_interface("lo0"));
        assert!(!is_counted_interface("utun3"));
        assert!(!is_counted_interface("awdl0"));
    }
}
