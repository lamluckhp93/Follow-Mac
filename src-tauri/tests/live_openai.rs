//! Live diagnostic against the real OpenAI API using the keys saved by the app.
//! Opt-in only: `cargo test --test live_openai -- --ignored --nocapture`.
//! Prints statuses, counts and field names — never the keys themselves.

use mac_observer_lib::ai::adapter;
use mac_observer_lib::ai::provider::{ProviderContext, ProviderKind};
use mac_observer_lib::security::keychain::{self, KeyKind};
use mac_observer_lib::security::redact;
use serde_json::Value;

const DAY: i64 = 86_400;

async fn raw(http: &reqwest::Client, key: &str, path: &str, q: &[(&str, String)]) -> (u16, Value) {
    let r = http
        .get(format!("https://api.openai.com{path}"))
        .bearer_auth(key)
        .query(q)
        .send()
        .await
        .expect("request");
    let status = r.status().as_u16();
    let v: Value = r.json().await.unwrap_or(Value::Null);
    (status, v)
}

fn describe(label: &str, status: u16, v: &Value, secrets: &[&str]) {
    let buckets = v.get("data").and_then(Value::as_array).cloned().unwrap_or_default();
    let results: Vec<&Value> = buckets.iter().filter_map(|b| b.get("results").and_then(Value::as_array)).flatten().collect();
    println!("[{label}] HTTP {status} · buckets={} · results={} · has_more={}", buckets.len(), results.len(), v.get("has_more").unwrap_or(&Value::Null));
    if let Some(err) = v.get("error") {
        println!("  error: {}", redact(&err.to_string(), secrets));
    }
    if let Some(first) = results.first() {
        let keys: Vec<&str> = first.as_object().map(|o| o.keys().map(String::as_str).collect()).unwrap_or_default();
        println!("  result fields: {keys:?}");
    }
    let sum = |f: &str| results.iter().filter_map(|r| r.get(f).and_then(Value::as_i64)).sum::<i64>();
    if !results.is_empty() {
        println!(
            "  sums: input={} cached={} output={} requests={}",
            sum("input_tokens"),
            sum("input_cached_tokens"),
            sum("output_tokens"),
            sum("num_model_requests")
        );
        let mut models: Vec<String> = results.iter().filter_map(|r| r.get("model").and_then(Value::as_str).map(String::from)).collect();
        models.sort();
        models.dedup();
        if !models.is_empty() {
            println!("  models: {models:?}");
        }
        let amount: f64 = results.iter().filter_map(|r| r.pointer("/amount/value").and_then(Value::as_f64)).sum();
        if amount > 0.0 {
            println!("  cost sum: ${amount:.4}");
        }
    }
}

#[tokio::test]
#[ignore = "calls the live OpenAI API with the user's Keychain credentials"]
async fn live_openai_diagnostic() {
    let api = keychain::get("openai", KeyKind::Api).expect("keychain");
    let admin = keychain::get("openai", KeyKind::Admin).expect("keychain");
    println!("api key present: {} · admin key present: {}", api.is_some(), admin.is_some());
    let secrets_owned: Vec<String> = [api.clone(), admin.clone()].into_iter().flatten().collect();
    let secrets: Vec<&str> = secrets_owned.iter().map(String::as_str).collect();
    let http = reqwest::Client::builder().timeout(std::time::Duration::from_secs(30)).build().unwrap();
    let ctx = ProviderContext { http: http.clone(), api_key: api.clone(), admin_key: admin.clone(), base_url: None };
    let a = adapter(ProviderKind::Openai);

    match a.test_connection(&ctx).await {
        Ok(t) => println!(
            "[app test_connection] ok={} · {} · models={} · usage_api={:?}",
            t.ok,
            redact(&t.message, &secrets),
            t.models.len(),
            t.usage_api.map(|u| (u.ok, redact(&u.message, &secrets)))
        ),
        Err(e) => println!("[app test_connection] ERROR {}", redact(&e.to_string(), &secrets)),
    }

    let now = chrono::Utc::now().timestamp();
    let from = now - 30 * DAY;
    if let Some(admin) = &admin {
        let base = vec![("start_time", from.to_string()), ("end_time", now.to_string()), ("bucket_width", "1d".to_string()), ("limit", "31".to_string())];
        let mut q = base.clone();
        q.push(("group_by", "model".into()));
        let (s, v) = raw(&http, admin, "/v1/organization/usage/completions", &q).await;
        describe("raw usage/completions group_by=model, 30d", s, &v, &secrets);
        let (s, v) = raw(&http, admin, "/v1/organization/usage/completions", &base).await;
        describe("raw usage/completions ungrouped, 30d", s, &v, &secrets);
        let mut q = base.clone();
        q.push(("group_by", "line_item".into()));
        let (s, v) = raw(&http, admin, "/v1/organization/costs", &q).await;
        describe("raw costs group_by=line_item, 30d", s, &v, &secrets);
    }

    match a.sync_usage(&ctx, from * 1000, now * 1000).await {
        Ok(r) => {
            println!("[app sync_usage] usage records={} · cost records={} · account={}", r.usage.len(), r.costs.len(), r.account.is_some());
            for n in &r.notes {
                println!("  note: {}", redact(n, &secrets));
            }
        }
        Err(e) => println!("[app sync_usage] ERROR {}", redact(&e.to_string(), &secrets)),
    }
}
