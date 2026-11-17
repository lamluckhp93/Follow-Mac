//! Manual import of usage data from CSV or JSON.
//!
//! Columns / keys (case-insensitive, aliases in brackets):
//!   date [timestamp, time, day]      RFC 3339, `YYYY-MM-DD`, `YYYY-MM-DD HH:MM:SS` (UTC) or Unix s/ms
//!   provider                         openai | anthropic | gemini | openrouter | deepseek | groq | custom
//!   model
//!   input_tokens [prompt_tokens, input]       uncached input tokens
//!   output_tokens [completion_tokens, output]
//!   cached_tokens [cache_read_tokens, cached_input_tokens]   optional
//!   cache_write_tokens [cache_creation_tokens]              optional
//!   requests                                                optional
//!   cost_usd [cost]                                         optional
//!
//! Each row gets a content hash as `external_id`, so importing the same file
//! twice does not double count.

use super::provider::{ProviderKind, UsageRecord};
use crate::error::{AppError, AppResult};
use crate::util::{utc_day_start, DAY_MS};
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::HashMap;

#[derive(Debug, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ImportReport {
    pub imported: usize,
    pub errors: Vec<ImportError>,
    pub total_rows: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportError {
    pub row: usize,
    pub message: String,
}

fn get<'a>(m: &'a HashMap<String, String>, keys: &[&str]) -> Option<&'a str> {
    keys.iter().find_map(|k| m.get(*k)).map(|s| s.trim()).filter(|s| !s.is_empty())
}

fn parse_int(m: &HashMap<String, String>, keys: &[&str]) -> Result<i64, String> {
    match get(m, keys) {
        None => Ok(0),
        Some(v) => {
            let cleaned: String = v.chars().filter(|c| *c != ',' && *c != '_').collect();
            let n: f64 = cleaned.parse().map_err(|_| format!("'{}' is not a number ({})", v, keys[0]))?;
            if n < 0.0 || !n.is_finite() {
                return Err(format!("{} must be ≥ 0", keys[0]));
            }
            Ok(n.round() as i64)
        }
    }
}

/// Returns (bucket_start, bucket_end) in ms.
pub fn parse_date(s: &str) -> Result<(i64, i64), String> {
    let s = s.trim();
    if let Ok(d) = chrono::DateTime::parse_from_rfc3339(s) {
        let t = d.timestamp_millis();
        return Ok((t, t));
    }
    if let Ok(d) = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S") {
        let t = d.and_utc().timestamp_millis();
        return Ok((t, t));
    }
    if let Ok(d) = chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        let t = d.and_hms_opt(0, 0, 0).unwrap().and_utc().timestamp_millis();
        return Ok((t, t + DAY_MS));
    }
    if let Ok(n) = s.parse::<i64>() {
        // Heuristic: values below 1e11 are seconds.
        let t = if n < 100_000_000_000 { n * 1000 } else { n };
        return Ok((t, t));
    }
    Err(format!("unrecognised date '{s}'"))
}

fn normalise_row(m: &HashMap<String, String>) -> Result<UsageRecord, String> {
    let date = get(m, &["date", "timestamp", "time", "day"]).ok_or("missing date")?;
    let (start, end) = parse_date(date)?;
    let provider = get(m, &["provider"]).ok_or("missing provider")?.to_lowercase();
    ProviderKind::parse(&provider).map_err(|_| format!("unknown provider '{provider}'"))?;
    let model = get(m, &["model"]).ok_or("missing model")?.to_string();
    let input = parse_int(m, &["input_tokens", "prompt_tokens", "input"])?;
    let output = parse_int(m, &["output_tokens", "completion_tokens", "output"])?;
    let cached = parse_int(m, &["cached_tokens", "cache_read_tokens", "cached_input_tokens"])?;
    let cache_write = parse_int(m, &["cache_write_tokens", "cache_creation_tokens"])?;
    let requests = parse_int(m, &["requests", "num_requests"])?;
    let cost = match get(m, &["cost_usd", "cost"]) {
        None => None,
        Some(v) => {
            let c: f64 = v.trim_start_matches('$').parse().map_err(|_| format!("'{v}' is not a valid cost"))?;
            if c < 0.0 || !c.is_finite() {
                return Err("cost must be ≥ 0".into());
            }
            Some(c)
        }
    };
    if input + output + cached + cache_write == 0 && cost.is_none() {
        return Err("row has no tokens and no cost".into());
    }
    let mut h = Sha256::new();
    h.update(format!("{provider}|{model}|{start}|{end}|{input}|{output}|{cached}|{cache_write}|{requests}|{cost:?}"));
    let id = format!("{:x}", h.finalize());
    Ok(UsageRecord {
        provider,
        model,
        source: "imported".into(),
        bucket_start: if end > start { utc_day_start(start) } else { start },
        bucket_end: end,
        input_tokens: input,
        output_tokens: output,
        cached_tokens: cached,
        cache_write_tokens: cache_write,
        reasoning_tokens: 0,
        requests,
        cost_usd: cost,
        cost_source: cost.map(|_| "imported".into()),
        external_id: id[..32].to_string(),
    })
}

fn lowercase_keys(m: HashMap<String, String>) -> HashMap<String, String> {
    m.into_iter().map(|(k, v)| (k.trim().to_lowercase().replace([' ', '-'], "_"), v)).collect()
}

pub fn parse(content: &str, format: &str) -> AppResult<(Vec<UsageRecord>, ImportReport)> {
    let mut report = ImportReport::default();
    let mut rows: Vec<HashMap<String, String>> = vec![];
    match format {
        "csv" => {
            let mut rdr = csv::ReaderBuilder::new().trim(csv::Trim::All).flexible(true).from_reader(content.as_bytes());
            let headers = rdr.headers().map_err(|e| AppError::Invalid(format!("CSV header: {e}")))?.clone();
            for rec in rdr.records() {
                let rec = rec.map_err(|e| AppError::Invalid(format!("CSV: {e}")))?;
                rows.push(lowercase_keys(headers.iter().zip(rec.iter()).map(|(h, v)| (h.to_string(), v.to_string())).collect()));
            }
        }
        "json" => {
            let v: Value = serde_json::from_str(content)?;
            let arr = match &v {
                Value::Array(a) => a.clone(),
                Value::Object(o) => o
                    .get("records")
                    .or_else(|| o.get("data"))
                    .and_then(Value::as_array)
                    .cloned()
                    .ok_or_else(|| AppError::Invalid("JSON must be an array or {\"records\": [...]}".into()))?,
                _ => return Err(AppError::Invalid("JSON must be an array of objects".into())),
            };
            for item in arr {
                let obj = item.as_object().ok_or_else(|| AppError::Invalid("JSON array items must be objects".into()))?;
                rows.push(lowercase_keys(
                    obj.iter()
                        .map(|(k, v)| (k.clone(), v.as_str().map(str::to_string).unwrap_or_else(|| if v.is_null() { String::new() } else { v.to_string() })))
                        .collect(),
                ));
            }
        }
        _ => return Err(AppError::Invalid("format must be csv or json".into())),
    }
    report.total_rows = rows.len();
    let mut out = vec![];
    for (i, r) in rows.iter().enumerate() {
        match normalise_row(r) {
            Ok(rec) => out.push(rec),
            Err(message) => {
                if report.errors.len() < 100 {
                    report.errors.push(ImportError { row: i + 1, message });
                }
            }
        }
    }
    report.imported = out.len();
    Ok((out, report))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_with_aliases_and_errors() {
        let csv = "Date,Provider,Model,Prompt Tokens,Completion Tokens,Cost\n\
                   2026-11-01,OpenAI,gpt-4o,\"1,000\",200,0.01\n\
                   2026-11-02T10:00:00Z,anthropic,claude-opus-5-5,10,5,\n\
                   bad-date,openai,x,1,1,\n\
                   2026-11-03,unknownco,x,1,1,\n";
        let (recs, rep) = parse(csv, "csv").unwrap();
        assert_eq!(rep.total_rows, 4);
        assert_eq!(recs.len(), 2);
        assert_eq!(rep.errors.len(), 2);
        assert_eq!(recs[0].input_tokens, 1000);
        assert_eq!(recs[0].cost_usd, Some(0.01));
        assert_eq!(recs[0].bucket_end - recs[0].bucket_start, DAY_MS);
        assert_eq!(recs[1].bucket_start, recs[1].bucket_end);
    }

    #[test]
    fn json_and_stable_ids() {
        let j = r#"[{"date":"2026-11-01","provider":"gemini","model":"gemini-2.5-pro","input_tokens":5,"output_tokens":7}]"#;
        let (a, _) = parse(j, "json").unwrap();
        let (b, _) = parse(j, "json").unwrap();
        assert_eq!(a[0].external_id, b[0].external_id);
        assert_eq!(a[0].source, "imported");
        let wrapped = r#"{"records":[{"date":1730419200,"provider":"groq","model":"m","input":1,"output":2}]}"#;
        assert_eq!(parse(wrapped, "json").unwrap().0.len(), 1);
    }

    #[test]
    fn rejects_negative() {
        let (r, rep) = parse("date,provider,model,input_tokens\n2026-01-01,openai,m,-5\n", "csv").unwrap();
        assert!(r.is_empty());
        assert!(rep.errors[0].message.contains("≥ 0"));
    }
}
