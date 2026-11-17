//! (1) Extract token usage from any provider response (JSON or SSE stream).
//! (2) Aggregate stored records into analytics with source de-duplication.
//!
//! Normalised token fields (all non-overlapping, so they can be summed):
//! * `input`       – uncached input/prompt tokens
//! * `cached`      – input tokens served from cache (cache reads)
//! * `cache_write` – input tokens written to cache (Anthropic)
//! * `output`      – output tokens (includes reasoning/thinking tokens)
//! * `reasoning`   – informational subset of `output`

use super::cost::Pricing;
use super::provider::{CostRecord, UsageRecord};
use crate::util::{utc_day_label, utc_day_start, utc_month_label};
use serde::Serialize;
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ExtractedUsage {
    pub model: Option<String>,
    pub input: i64,
    pub output: i64,
    pub cached: i64,
    pub cache_write: i64,
    pub reasoning: i64,
    pub cost_usd: Option<f64>,
}

fn n(v: &Value, p: &str) -> i64 {
    v.pointer(p).and_then(Value::as_i64).unwrap_or(0)
}

/// Extract usage from one JSON object (full response or a single stream event).
/// `provider` selects semantics where formats are ambiguous.
pub fn extract(v: &Value, provider: &str) -> Option<ExtractedUsage> {
    if let Some(arr) = v.as_array() {
        // Gemini streamGenerateContent without SSE returns a JSON array.
        return arr.iter().rev().find_map(|e| extract(e, provider));
    }
    let model = v
        .get("model")
        .or_else(|| v.pointer("/message/model"))
        .or_else(|| v.pointer("/response/model"))
        .or_else(|| v.get("modelVersion"))
        .and_then(Value::as_str)
        .map(str::to_string);

    // Gemini
    if let Some(u) = v.get("usageMetadata") {
        let prompt = n(u, "/promptTokenCount");
        let cached = n(u, "/cachedContentTokenCount");
        let thoughts = n(u, "/thoughtsTokenCount");
        return Some(ExtractedUsage {
            model,
            input: (prompt - cached).max(0),
            cached,
            output: n(u, "/candidatesTokenCount") + thoughts,
            reasoning: thoughts,
            ..Default::default()
        });
    }

    // Anthropic: message, message_start (usage under message), message_delta
    if provider == "anthropic" {
        let u = v.get("usage").or_else(|| v.pointer("/message/usage"))?;
        return Some(ExtractedUsage {
            model,
            input: n(u, "/input_tokens"),
            output: n(u, "/output_tokens"),
            cached: n(u, "/cache_read_input_tokens"),
            cache_write: n(u, "/cache_creation_input_tokens"),
            ..Default::default()
        });
    }

    // OpenAI-compatible (chat completions, responses API, embeddings), incl.
    // Groq stream `x_groq.usage` and Responses API `response.completed`.
    let u = v
        .get("usage")
        .filter(|u| u.is_object())
        .or_else(|| v.pointer("/x_groq/usage"))
        .or_else(|| v.pointer("/response/usage"))
        .filter(|u| u.is_object())?;
    let (prompt, completion) = if u.get("prompt_tokens").is_some() || u.get("completion_tokens").is_some() {
        (n(u, "/prompt_tokens"), n(u, "/completion_tokens"))
    } else {
        (n(u, "/input_tokens"), n(u, "/output_tokens"))
    };
    let cached = [
        n(u, "/prompt_tokens_details/cached_tokens"),
        n(u, "/input_tokens_details/cached_tokens"),
        n(u, "/prompt_cache_hit_tokens"), // DeepSeek
    ]
    .into_iter()
    .max()
    .unwrap_or(0);
    let reasoning = n(u, "/completion_tokens_details/reasoning_tokens").max(n(u, "/output_tokens_details/reasoning_tokens"));
    Some(ExtractedUsage {
        model,
        input: (prompt - cached).max(0),
        output: completion,
        cached,
        cache_write: 0,
        reasoning,
        cost_usd: u.get("cost").and_then(Value::as_f64), // OpenRouter
    })
}

/// Incremental usage collector for a response body (handles SSE streams).
pub struct UsageAccumulator {
    provider: String,
    sse: bool,
    line_buf: Vec<u8>,
    body: Vec<u8>,
    best: Option<ExtractedUsage>,
    overflow: bool,
}

const MAX_BODY: usize = 32 * 1024 * 1024;

impl UsageAccumulator {
    pub fn new(provider: &str, content_type: &str) -> Self {
        Self {
            provider: provider.to_string(),
            sse: content_type.contains("text/event-stream"),
            line_buf: vec![],
            body: vec![],
            best: None,
            overflow: false,
        }
    }

    fn merge(&mut self, u: ExtractedUsage) {
        let b = self.best.get_or_insert_with(Default::default);
        if b.model.is_none() {
            b.model = u.model;
        }
        // Stream events carry cumulative counts → keep the max of each field.
        b.input = b.input.max(u.input);
        b.output = b.output.max(u.output);
        b.cached = b.cached.max(u.cached);
        b.cache_write = b.cache_write.max(u.cache_write);
        b.reasoning = b.reasoning.max(u.reasoning);
        if u.cost_usd.is_some() {
            b.cost_usd = u.cost_usd;
        }
    }

    pub fn feed(&mut self, chunk: &[u8]) {
        if !self.sse {
            if self.body.len() + chunk.len() <= MAX_BODY {
                self.body.extend_from_slice(chunk);
            } else {
                self.overflow = true;
            }
            return;
        }
        self.line_buf.extend_from_slice(chunk);
        while let Some(pos) = self.line_buf.iter().position(|b| *b == b'\n') {
            let line: Vec<u8> = self.line_buf.drain(..=pos).collect();
            self.handle_line(&line);
        }
        if self.line_buf.len() > MAX_BODY {
            self.line_buf.clear();
        }
    }

    fn handle_line(&mut self, line: &[u8]) {
        let Ok(s) = std::str::from_utf8(line) else { return };
        let s = s.trim();
        let Some(data) = s.strip_prefix("data:") else { return };
        let data = data.trim();
        if data.is_empty() || data == "[DONE]" {
            return;
        }
        if let Ok(v) = serde_json::from_str::<Value>(data) {
            if let Some(u) = extract(&v, &self.provider) {
                self.merge(u);
            }
        }
    }

    pub fn finish(mut self) -> Option<ExtractedUsage> {
        if self.sse {
            let rest = std::mem::take(&mut self.line_buf);
            self.handle_line(&rest);
        } else if !self.overflow {
            if let Ok(v) = serde_json::from_slice::<Value>(&self.body) {
                if let Some(u) = extract(&v, &self.provider) {
                    self.merge(u);
                }
            }
        }
        self.best.filter(|b| b.input + b.output + b.cached + b.cache_write > 0)
    }
}

// ---------------------------------------------------------------------------
// Aggregation
// ---------------------------------------------------------------------------

pub fn source_rank(source: &str) -> u8 {
    match source {
        "provider_reported" => 0,
        "imported" => 1,
        "telemetry" => 2,
        _ => 9,
    }
}

#[derive(Debug, Clone, Serialize, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Totals {
    pub input: i64,
    pub output: i64,
    pub cached: i64,
    pub cache_write: i64,
    pub reasoning: i64,
    pub requests: i64,
    pub total: i64,
    pub actual_cost: f64,
    pub estimated_cost: f64,
    /// Tokens for which no actual cost exists and no price is configured.
    pub unpriced_tokens: i64,
}

impl Totals {
    fn add_tokens(&mut self, r: &UsageRecord) {
        self.input += r.input_tokens;
        self.output += r.output_tokens;
        self.cached += r.cached_tokens;
        self.cache_write += r.cache_write_tokens;
        self.reasoning += r.reasoning_tokens;
        self.requests += r.requests;
        self.total += r.input_tokens + r.output_tokens + r.cached_tokens + r.cache_write_tokens;
    }
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Breakdown {
    pub key: String,
    pub provider: String,
    pub totals: Totals,
    pub sources: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SeriesPoint {
    pub label: String,
    pub ts: i64,
    pub tokens_by_provider: BTreeMap<String, i64>,
    pub input: i64,
    pub output: i64,
    pub cached: i64,
    pub actual_cost: f64,
    pub estimated_cost: f64,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Analytics {
    pub from: i64,
    pub to: i64,
    pub totals: Totals,
    pub by_provider: Vec<Breakdown>,
    pub by_model: Vec<Breakdown>,
    pub daily: Vec<SeriesPoint>,
    pub monthly: Vec<SeriesPoint>,
    pub record_count: usize,
    pub deduplicated_records: usize,
}

/// `forced_source`: `None` = automatic best-source-per-(provider, UTC day).
pub fn aggregate(
    records: &[UsageRecord],
    costs: &[CostRecord],
    pricing: &Pricing,
    from: i64,
    to: i64,
    forced_source: Option<&str>,
) -> Analytics {
    // 1. Pick one source per (provider, day).
    let mut best: HashMap<(String, i64), u8> = HashMap::new();
    for r in records {
        if forced_source.is_some_and(|f| f != r.source) {
            continue;
        }
        let k = (r.provider.clone(), utc_day_start(r.bucket_start));
        let rank = source_rank(&r.source);
        best.entry(k).and_modify(|b| *b = (*b).min(rank)).or_insert(rank);
    }
    let kept: Vec<&UsageRecord> = records
        .iter()
        .filter(|r| best.get(&(r.provider.clone(), utc_day_start(r.bucket_start))) == Some(&source_rank(&r.source)))
        .filter(|r| forced_source.is_none_or(|f| f == r.source))
        .collect();

    // 2. Cost records grouped by (provider, day).
    let mut day_costs: HashMap<(String, i64), Vec<&CostRecord>> = HashMap::new();
    for c in costs {
        day_costs.entry((c.provider.clone(), utc_day_start(c.bucket_start))).or_default().push(c);
    }

    let mut a = Analytics { from, to, record_count: records.len(), deduplicated_records: records.len() - kept.len(), ..Default::default() };
    let mut by_provider: BTreeMap<String, Breakdown> = BTreeMap::new();
    let mut by_model: BTreeMap<(String, String), Breakdown> = BTreeMap::new();
    let mut daily: BTreeMap<i64, SeriesPoint> = BTreeMap::new();

    let add_source = |b: &mut Breakdown, s: &str| {
        if !b.sources.iter().any(|x| x == s) {
            b.sources.push(s.to_string());
        }
    };

    for r in &kept {
        let day = utc_day_start(r.bucket_start);
        let has_cost_records = day_costs.contains_key(&(r.provider.clone(), day));
        let (mut actual, mut estimated, mut unpriced) = (0.0, 0.0, 0i64);
        if !has_cost_records {
            if let Some(c) = r.cost_usd {
                actual = c;
            } else if let Some(e) = pricing.estimate(r) {
                estimated = e;
            } else {
                unpriced = r.input_tokens + r.output_tokens + r.cached_tokens + r.cache_write_tokens;
            }
        }
        for t in [
            &mut a.totals,
            &mut by_provider
                .entry(r.provider.clone())
                .or_insert_with(|| Breakdown { key: r.provider.clone(), provider: r.provider.clone(), ..Default::default() })
                .totals,
            &mut by_model
                .entry((r.provider.clone(), r.model.clone()))
                .or_insert_with(|| Breakdown { key: r.model.clone(), provider: r.provider.clone(), ..Default::default() })
                .totals,
        ] {
            t.add_tokens(r);
            t.actual_cost += actual;
            t.estimated_cost += estimated;
            t.unpriced_tokens += unpriced;
        }
        add_source(by_provider.get_mut(&r.provider).unwrap(), &r.source);
        add_source(by_model.get_mut(&(r.provider.clone(), r.model.clone())).unwrap(), &r.source);
        let p = daily.entry(day).or_insert_with(|| SeriesPoint { label: utc_day_label(day), ts: day, ..Default::default() });
        *p.tokens_by_provider.entry(r.provider.clone()).or_default() +=
            r.input_tokens + r.output_tokens + r.cached_tokens + r.cache_write_tokens;
        p.input += r.input_tokens + r.cache_write_tokens;
        p.output += r.output_tokens;
        p.cached += r.cached_tokens;
        p.actual_cost += actual;
        p.estimated_cost += estimated;
    }

    // 3. Provider-reported / imported cost records (authoritative for their day).
    for ((provider, day), list) in &day_costs {
        for c in list {
            a.totals.actual_cost += c.amount_usd;
            let pb = by_provider
                .entry(provider.clone())
                .or_insert_with(|| Breakdown { key: provider.clone(), provider: provider.clone(), ..Default::default() });
            pb.totals.actual_cost += c.amount_usd;
            add_source(pb, &format!("{}_cost", c.source));
            let mk = c.model.clone().unwrap_or_else(|| "(not attributed to a model)".into());
            let mb = by_model
                .entry((provider.clone(), mk.clone()))
                .or_insert_with(|| Breakdown { key: mk, provider: provider.clone(), ..Default::default() });
            mb.totals.actual_cost += c.amount_usd;
            let p = daily.entry(*day).or_insert_with(|| SeriesPoint { label: utc_day_label(*day), ts: *day, ..Default::default() });
            p.actual_cost += c.amount_usd;
        }
    }

    // 4. Monthly roll-up from daily.
    let mut monthly: BTreeMap<String, SeriesPoint> = BTreeMap::new();
    for d in daily.values() {
        let label = utc_month_label(d.ts);
        let m = monthly.entry(label.clone()).or_insert_with(|| SeriesPoint { label, ts: d.ts, ..Default::default() });
        for (k, v) in &d.tokens_by_provider {
            *m.tokens_by_provider.entry(k.clone()).or_default() += v;
        }
        m.input += d.input;
        m.output += d.output;
        m.cached += d.cached;
        m.actual_cost += d.actual_cost;
        m.estimated_cost += d.estimated_cost;
    }

    let mut bp: Vec<Breakdown> = by_provider.into_values().collect();
    bp.sort_by(|a, b| b.totals.total.cmp(&a.totals.total));
    let mut bm: Vec<Breakdown> = by_model.into_values().collect();
    bm.sort_by(|a, b| {
        b.totals.total.cmp(&a.totals.total).then(
            (b.totals.actual_cost + b.totals.estimated_cost)
                .partial_cmp(&(a.totals.actual_cost + a.totals.estimated_cost))
                .unwrap_or(std::cmp::Ordering::Equal),
        )
    });
    a.by_provider = bp;
    a.by_model = bm;
    a.daily = daily.into_values().collect();
    a.monthly = monthly.into_values().collect();
    a
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::cost::{ModelPrice, Pricing};
    use serde_json::json;

    #[test]
    fn openai_chat_usage() {
        let v = json!({"model":"gpt-4o-mini","usage":{"prompt_tokens":100,"completion_tokens":20,
            "prompt_tokens_details":{"cached_tokens":40},"completion_tokens_details":{"reasoning_tokens":5}}});
        let u = extract(&v, "openai").unwrap();
        assert_eq!((u.input, u.cached, u.output, u.reasoning), (60, 40, 20, 5));
    }

    #[test]
    fn openai_responses_api_stream_completed() {
        let v = json!({"type":"response.completed","response":{"model":"gpt-5","usage":{"input_tokens":50,
            "input_tokens_details":{"cached_tokens":10},"output_tokens":7,"output_tokens_details":{"reasoning_tokens":3}}}});
        let u = extract(&v, "openai").unwrap();
        assert_eq!((u.input, u.cached, u.output, u.model.as_deref()), (40, 10, 7, Some("gpt-5")));
    }

    #[test]
    fn anthropic_message() {
        let v = json!({"type":"message","model":"claude-opus-5-5","usage":{"input_tokens":12,"output_tokens":6,
            "cache_read_input_tokens":100,"cache_creation_input_tokens":50}});
        let u = extract(&v, "anthropic").unwrap();
        assert_eq!((u.input, u.output, u.cached, u.cache_write), (12, 6, 100, 50));
    }

    #[test]
    fn anthropic_sse_stream_merges_events() {
        let mut acc = UsageAccumulator::new("anthropic", "text/event-stream; charset=utf-8");
        let stream = concat!(
            "event: message_start\n",
            "data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_1\",\"model\":\"claude-sonnet-5-5\",\"usage\":{\"input_tokens\":25,\"output_tokens\":1,\"cache_read_input_tokens\":0}}}\n\n",
            "event: content_block_delta\n",
            "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"po\"}}\n\n",
            "event: message_delta\n",
            "data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"output_tokens\":12}}\n\n",
        );
        // Feed in awkward chunk sizes to exercise line buffering.
        for chunk in stream.as_bytes().chunks(7) {
            acc.feed(chunk);
        }
        let u = acc.finish().unwrap();
        assert_eq!((u.input, u.output, u.model.as_deref()), (25, 12, Some("claude-sonnet-5-5")));
    }

    #[test]
    fn openai_sse_with_include_usage() {
        let mut acc = UsageAccumulator::new("openai", "text/event-stream");
        acc.feed(b"data: {\"model\":\"gpt-4o\",\"choices\":[{\"delta\":{\"content\":\"hi\"}}],\"usage\":null}\n\n");
        acc.feed(b"data: {\"model\":\"gpt-4o\",\"choices\":[],\"usage\":{\"prompt_tokens\":9,\"completion_tokens\":3}}\n\ndata: [DONE]\n\n");
        let u = acc.finish().unwrap();
        assert_eq!((u.input, u.output), (9, 3));
    }

    #[test]
    fn gemini_usage_metadata() {
        let v = json!({"candidates":[],"modelVersion":"gemini-2.5-flash","usageMetadata":{"promptTokenCount":30,
            "candidatesTokenCount":10,"cachedContentTokenCount":5,"thoughtsTokenCount":4,"totalTokenCount":44}});
        let u = extract(&v, "gemini").unwrap();
        assert_eq!((u.input, u.cached, u.output, u.reasoning), (25, 5, 14, 4));
        let mut acc = UsageAccumulator::new("gemini", "application/json");
        acc.feed(serde_json::to_string(&json!([v.clone(), v])).unwrap().as_bytes());
        assert_eq!(acc.finish().unwrap().input, 25);
    }

    #[test]
    fn deepseek_and_openrouter_fields() {
        let v = json!({"model":"deepseek-chat","usage":{"prompt_tokens":100,"completion_tokens":10,"prompt_cache_hit_tokens":80,"prompt_cache_miss_tokens":20}});
        let u = extract(&v, "deepseek").unwrap();
        assert_eq!((u.input, u.cached), (20, 80));
        let v = json!({"model":"x","usage":{"prompt_tokens":1,"completion_tokens":1,"cost":0.0012}});
        assert_eq!(extract(&v, "openrouter").unwrap().cost_usd, Some(0.0012));
    }

    #[test]
    fn no_usage_returns_none() {
        assert!(extract(&json!({"object":"list","data":[]}), "openai").is_none());
        let acc = UsageAccumulator::new("openai", "application/json");
        assert!(acc.finish().is_none());
    }

    fn rec(provider: &str, model: &str, source: &str, day: i64, input: i64, output: i64, id: &str) -> UsageRecord {
        UsageRecord {
            provider: provider.into(),
            model: model.into(),
            source: source.into(),
            bucket_start: day * 86_400_000 + 3_600_000,
            bucket_end: day * 86_400_000 + 3_600_000,
            input_tokens: input,
            output_tokens: output,
            cached_tokens: 0,
            cache_write_tokens: 0,
            reasoning_tokens: 0,
            requests: 1,
            cost_usd: None,
            cost_source: None,
            external_id: id.into(),
        }
    }

    fn pricing() -> Pricing {
        Pricing::new(vec![ModelPrice {
            provider: "anthropic".into(),
            model: "claude-opus-5-5".into(),
            input_price: Some(4.0),
            output_price: Some(20.0),
            cached_input_price: Some(0.2),
            cache_write_price: Some(5.0),
            price_source: "test".into(),
            updated_at: 0,
        }])
    }

    #[test]
    fn dedup_prefers_provider_reported_per_day() {
        let recs = vec![
            rec("anthropic", "claude-opus-5-5", "telemetry", 20000, 100, 10, "t1"),
            rec("anthropic", "claude-opus-5-5", "provider_reported", 20000, 1000, 100, "p1"),
            rec("anthropic", "claude-opus-5-5", "telemetry", 20001, 50, 5, "t2"), // no official data that day
        ];
        let a = aggregate(&recs, &[], &pricing(), 0, i64::MAX, None);
        assert_eq!(a.totals.input, 1050);
        assert_eq!(a.totals.output, 105);
        assert_eq!(a.deduplicated_records, 1);
        // estimated cost: (1050*4 + 105*20) / 1e6
        assert!((a.totals.estimated_cost - (1050.0 * 4.0 + 105.0 * 20.0) / 1e6).abs() < 1e-12);
        assert_eq!(a.daily.len(), 2);
    }

    #[test]
    fn cost_records_override_estimates() {
        let recs = vec![rec("anthropic", "claude-opus-5-5", "provider_reported", 20000, 1_000_000, 0, "p1")];
        let costs = vec![CostRecord {
            provider: "anthropic".into(),
            model: Some("claude-opus-5-5".into()),
            source: "provider_reported".into(),
            bucket_start: 20000 * 86_400_000,
            bucket_end: 20001 * 86_400_000,
            amount_usd: 3.5,
            line_item: None,
            external_id: "c1".into(),
        }];
        let a = aggregate(&recs, &costs, &pricing(), 0, i64::MAX, None);
        assert_eq!(a.totals.estimated_cost, 0.0);
        assert!((a.totals.actual_cost - 3.5).abs() < 1e-12);
        assert!((a.by_model[0].totals.actual_cost - 3.5).abs() < 1e-12);
    }

    #[test]
    fn unpriced_tokens_are_reported_not_guessed() {
        let recs = vec![rec("groq", "llama-x", "telemetry", 20000, 10, 5, "t")];
        let a = aggregate(&recs, &[], &pricing(), 0, i64::MAX, None);
        assert_eq!(a.totals.estimated_cost, 0.0);
        assert_eq!(a.totals.unpriced_tokens, 15);
    }

    #[test]
    fn forced_source_filters() {
        let recs = vec![
            rec("openai", "m", "telemetry", 20000, 1, 1, "t"),
            rec("openai", "m", "provider_reported", 20000, 10, 10, "p"),
        ];
        let a = aggregate(&recs, &[], &Pricing::new(vec![]), 0, i64::MAX, Some("telemetry"));
        assert_eq!(a.totals.input, 1);
    }
}
