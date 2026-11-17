//! Cost estimation from the user-editable pricing table (`ai_models`).
//! Estimates are always labelled "Estimated" in the UI; actual cost only comes
//! from provider cost APIs or response metadata.

use super::provider::UsageRecord;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ModelPrice {
    pub provider: String,
    pub model: String,
    /// USD per 1M tokens
    pub input_price: Option<f64>,
    pub output_price: Option<f64>,
    pub cached_input_price: Option<f64>,
    pub cache_write_price: Option<f64>,
    pub price_source: String,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Default)]
pub struct Pricing {
    prices: Vec<ModelPrice>,
}

/// Normalise model ids: drop `models/` (Gemini) and `vendor/` (OpenRouter) prefixes.
fn normalise(model: &str) -> &str {
    let m = model.trim_start_matches("models/");
    m.rsplit_once('/').map(|(_, r)| r).unwrap_or(m)
}

impl Pricing {
    pub fn new(prices: Vec<ModelPrice>) -> Self {
        Self { prices }
    }

    /// Exact match first, then the longest configured id that prefixes the
    /// usage model followed by `-` or `@` (e.g. `gpt-4o` matches
    /// `gpt-4o-2024-08-06`, but not `gpt-4o-mini` when `gpt-4o-mini` exists).
    pub fn find(&self, provider: &str, model: &str) -> Option<&ModelPrice> {
        let m = normalise(model);
        let candidates = self.prices.iter().filter(|p| p.provider == provider || provider == "openrouter");
        let mut best: Option<&ModelPrice> = None;
        for p in candidates {
            let pm = normalise(&p.model);
            if pm == m {
                // Prefer an exact provider match over an OpenRouter cross-provider match.
                if p.provider == provider {
                    return Some(p);
                }
                best = Some(p);
                continue;
            }
            let is_prefix = m.len() > pm.len()
                && m.starts_with(pm)
                && matches!(m.as_bytes()[pm.len()], b'-' | b'@' | b':');
            if is_prefix && best.is_none_or(|b| normalise(&b.model).len() < pm.len()) {
                best = Some(p);
            }
        }
        best.filter(|p| p.input_price.is_some() || p.output_price.is_some())
    }

    pub fn estimate(&self, r: &UsageRecord) -> Option<f64> {
        let p = self.find(&r.provider, &r.model)?;
        let input = p.input_price?;
        let output = p.output_price?;
        let cached = p.cached_input_price.unwrap_or(input);
        let write = p.cache_write_price.unwrap_or(input);
        Some(
            (r.input_tokens as f64 * input
                + r.output_tokens as f64 * output
                + r.cached_tokens as f64 * cached
                + r.cache_write_tokens as f64 * write)
                / 1_000_000.0,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn price(provider: &str, model: &str, i: f64, o: f64) -> ModelPrice {
        ModelPrice {
            provider: provider.into(),
            model: model.into(),
            input_price: Some(i),
            output_price: Some(o),
            cached_input_price: None,
            cache_write_price: None,
            price_source: "test".into(),
            updated_at: 0,
        }
    }

    #[test]
    fn matching_rules() {
        let p = Pricing::new(vec![price("openai", "gpt-4o", 1.0, 2.0), price("openai", "gpt-4o-mini", 0.1, 0.2)]);
        assert_eq!(p.find("openai", "gpt-4o-2024-08-06").unwrap().model, "gpt-4o");
        assert_eq!(p.find("openai", "gpt-4o-mini-2024-07-18").unwrap().model, "gpt-4o-mini");
        assert!(p.find("openai", "gpt-4").is_none());
        assert!(p.find("anthropic", "gpt-4o").is_none());
        assert_eq!(p.find("openrouter", "openai/gpt-4o").unwrap().model, "gpt-4o");
    }

    #[test]
    fn estimate_math() {
        let p = Pricing::new(vec![price("anthropic", "claude-opus-5-5", 4.0, 20.0)]);
        let r = UsageRecord {
            provider: "anthropic".into(),
            model: "claude-opus-5-5".into(),
            source: "telemetry".into(),
            bucket_start: 0,
            bucket_end: 0,
            input_tokens: 500_000,
            output_tokens: 100_000,
            cached_tokens: 0,
            cache_write_tokens: 0,
            reasoning_tokens: 0,
            requests: 1,
            cost_usd: None,
            cost_source: None,
            external_id: "x".into(),
        };
        assert!((p.estimate(&r).unwrap() - 4.0).abs() < 1e-9); // 2.0 + 2.0
    }
}
