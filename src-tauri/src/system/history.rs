//! Persistence of downsampled system metrics and time-range queries.

use super::SystemSnapshot;
use crate::database::Db;
use crate::error::AppResult;
use serde::Serialize;

#[derive(Debug, Clone, Serialize, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MetricPoint {
    pub ts: i64,
    pub cpu_usage: Option<f64>,
    pub cpu_user: Option<f64>,
    pub cpu_system: Option<f64>,
    pub mem_used: Option<f64>,
    pub mem_total: Option<f64>,
    pub mem_pressure: Option<f64>,
    pub swap_used: Option<f64>,
    pub gpu_usage: Option<f64>,
    pub disk_read_bps: Option<f64>,
    pub disk_write_bps: Option<f64>,
    pub net_rx_bps: Option<f64>,
    pub net_tx_bps: Option<f64>,
    pub battery_percent: Option<f64>,
    pub battery_charging: Option<f64>,
    pub cpu_temp: Option<f64>,
}

impl MetricPoint {
    pub fn from_snapshot(s: &SystemSnapshot) -> Self {
        let f = |v: Option<f32>| v.map(f64::from);
        let u = |v: Option<u64>| v.map(|x| x as f64);
        MetricPoint {
            ts: s.ts,
            cpu_usage: f(s.cpu.usage),
            cpu_user: f(s.cpu.user),
            cpu_system: f(s.cpu.system),
            mem_used: u(s.memory.used),
            mem_total: u(s.memory.total),
            mem_pressure: s.memory.pressure_level.map(f64::from),
            swap_used: u(s.memory.swap_used),
            gpu_usage: f(s.gpu.usage),
            disk_read_bps: s.disk.read_bps,
            disk_write_bps: s.disk.write_bps,
            net_rx_bps: s.network.rx_bps,
            net_tx_bps: s.network.tx_bps,
            battery_percent: f(s.battery.percent),
            battery_charging: s.battery.charging.map(|c| if c { 1.0 } else { 0.0 }),
            cpu_temp: f(s.temperature.cpu_c),
        }
    }

    fn fields_mut(&mut self) -> [&mut Option<f64>; 15] {
        [
            &mut self.cpu_usage,
            &mut self.cpu_user,
            &mut self.cpu_system,
            &mut self.mem_used,
            &mut self.mem_total,
            &mut self.mem_pressure,
            &mut self.swap_used,
            &mut self.gpu_usage,
            &mut self.disk_read_bps,
            &mut self.disk_write_bps,
            &mut self.net_rx_bps,
            &mut self.net_tx_bps,
            &mut self.battery_percent,
            &mut self.battery_charging,
            &mut self.cpu_temp,
        ]
    }

    fn fields(&self) -> [Option<f64>; 15] {
        [
            self.cpu_usage,
            self.cpu_user,
            self.cpu_system,
            self.mem_used,
            self.mem_total,
            self.mem_pressure,
            self.swap_used,
            self.gpu_usage,
            self.disk_read_bps,
            self.disk_write_bps,
            self.net_rx_bps,
            self.net_tx_bps,
            self.battery_percent,
            self.battery_charging,
            self.cpu_temp,
        ]
    }
}

/// Average a set of points field-by-field, ignoring missing values.
/// Memory pressure takes the max (a single critical sample matters).
pub fn average(points: &[MetricPoint], ts: i64) -> MetricPoint {
    let mut out = MetricPoint { ts, ..Default::default() };
    let mut sums = [0f64; 15];
    let mut counts = [0usize; 15];
    let mut max_pressure: Option<f64> = None;
    for p in points {
        for (i, v) in p.fields().iter().enumerate() {
            if let Some(v) = v {
                sums[i] += v;
                counts[i] += 1;
            }
        }
        if let Some(pr) = p.mem_pressure {
            max_pressure = Some(max_pressure.map_or(pr, |m: f64| m.max(pr)));
        }
    }
    for (i, f) in out.fields_mut().into_iter().enumerate() {
        *f = (counts[i] > 0).then(|| sums[i] / counts[i] as f64);
    }
    out.mem_pressure = max_pressure;
    out
}

/// Bucket points into at most `max_points` buckets over [from, to].
pub fn bucketize(points: &[MetricPoint], from: i64, to: i64, max_points: usize) -> Vec<MetricPoint> {
    if points.is_empty() || to <= from || max_points == 0 {
        return vec![];
    }
    // +1 so a point exactly at `to` still lands in the last bucket.
    let bucket = ((to - from + 1) as f64 / max_points as f64).ceil().max(1.0) as i64;
    let mut out = Vec::new();
    let mut cur: Vec<MetricPoint> = Vec::new();
    let mut cur_idx = None;
    for p in points.iter().filter(|p| p.ts >= from && p.ts <= to) {
        let idx = (p.ts - from) / bucket;
        if cur_idx.is_some() && cur_idx != Some(idx) {
            let i = cur_idx.unwrap();
            out.push(average(&cur, from + i * bucket + bucket / 2));
            cur.clear();
        }
        cur_idx = Some(idx);
        cur.push(p.clone());
    }
    if let Some(i) = cur_idx {
        out.push(average(&cur, from + i * bucket + bucket / 2));
    }
    out
}

pub fn insert_batch(db: &Db, rows: &[MetricPoint]) -> AppResult<()> {
    if rows.is_empty() {
        return Ok(());
    }
    db.call_sync(|c| {
        let tx = c.transaction()?;
        {
            let mut st = tx.prepare_cached(
                "INSERT INTO system_metrics (ts, cpu_usage, cpu_user, cpu_system, mem_used, mem_total, mem_pressure,
                 swap_used, gpu_usage, disk_read_bps, disk_write_bps, net_rx_bps, net_tx_bps, battery_percent,
                 battery_charging, cpu_temp) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16)",
            )?;
            for r in rows {
                st.execute(rusqlite::params![
                    r.ts,
                    r.cpu_usage,
                    r.cpu_user,
                    r.cpu_system,
                    r.mem_used.map(|v| v as i64),
                    r.mem_total.map(|v| v as i64),
                    r.mem_pressure.map(|v| v.round() as i64),
                    r.swap_used.map(|v| v as i64),
                    r.gpu_usage,
                    r.disk_read_bps,
                    r.disk_write_bps,
                    r.net_rx_bps,
                    r.net_tx_bps,
                    r.battery_percent,
                    r.battery_charging.map(|v| v.round() as i64),
                    r.cpu_temp,
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    })
}

pub async fn query_range(db: &Db, from: i64, to: i64) -> AppResult<Vec<MetricPoint>> {
    db.call(move |c| {
        let mut st = c.prepare_cached(
            "SELECT ts, cpu_usage, cpu_user, cpu_system, mem_used, mem_total, mem_pressure, swap_used, gpu_usage,
                    disk_read_bps, disk_write_bps, net_rx_bps, net_tx_bps, battery_percent, battery_charging, cpu_temp
             FROM system_metrics WHERE ts >= ?1 AND ts <= ?2 ORDER BY ts",
        )?;
        let rows = st
            .query_map([from, to], |r| {
                Ok(MetricPoint {
                    ts: r.get(0)?,
                    cpu_usage: r.get(1)?,
                    cpu_user: r.get(2)?,
                    cpu_system: r.get(3)?,
                    mem_used: r.get::<_, Option<i64>>(4)?.map(|v| v as f64),
                    mem_total: r.get::<_, Option<i64>>(5)?.map(|v| v as f64),
                    mem_pressure: r.get::<_, Option<i64>>(6)?.map(|v| v as f64),
                    swap_used: r.get::<_, Option<i64>>(7)?.map(|v| v as f64),
                    gpu_usage: r.get(8)?,
                    disk_read_bps: r.get(9)?,
                    disk_write_bps: r.get(10)?,
                    net_rx_bps: r.get(11)?,
                    net_tx_bps: r.get(12)?,
                    battery_percent: r.get(13)?,
                    battery_charging: r.get::<_, Option<i64>>(14)?.map(|v| v as f64),
                    cpu_temp: r.get(15)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(ts: i64, cpu: f64) -> MetricPoint {
        MetricPoint { ts, cpu_usage: Some(cpu), mem_pressure: Some(1.0), ..Default::default() }
    }

    #[test]
    fn average_ignores_missing() {
        let mut a = p(0, 10.0);
        a.gpu_usage = Some(50.0);
        let b = p(1, 30.0);
        let avg = average(&[a, b], 5);
        assert_eq!(avg.cpu_usage, Some(20.0));
        assert_eq!(avg.gpu_usage, Some(50.0));
        assert_eq!(avg.net_rx_bps, None);
    }

    #[test]
    fn pressure_uses_max() {
        let mut a = p(0, 1.0);
        a.mem_pressure = Some(4.0);
        assert_eq!(average(&[a, p(1, 1.0)], 0).mem_pressure, Some(4.0));
    }

    #[test]
    fn bucketize_limits_points() {
        let pts: Vec<_> = (0..1000).map(|i| p(i * 1000, i as f64)).collect();
        let out = bucketize(&pts, 0, 999_000, 100);
        assert!(out.len() <= 100 && out.len() >= 99);
        assert!(out.windows(2).all(|w| w[0].ts < w[1].ts));
    }

    #[tokio::test]
    async fn insert_and_query() {
        let db = Db::open_in_memory().unwrap();
        insert_batch(&db, &[p(1000, 5.0), p(2000, 7.0), p(5000, 9.0)]).unwrap();
        let rows = query_range(&db, 1500, 6000).await.unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].cpu_usage, Some(7.0));
    }
}
