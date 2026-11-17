//! Live diagnostic against the real Anthropic API using the keys saved by the app.
//! Opt-in only: `cargo test --test live_anthropic -- --ignored --nocapture`.
//! Prints statuses and counts — never the keys themselves. Makes no paid calls.

use mac_observer_lib::ai::adapter;
use mac_observer_lib::ai::provider::{ProviderContext, ProviderKind};
use mac_observer_lib::security::keychain::{self, KeyKind};
use mac_observer_lib::security::redact;

#[tokio::test]
#[ignore = "calls the live Anthropic API with the user's Keychain credentials"]
async fn live_anthropic_diagnostic() {
    let api = keychain::get("anthropic", KeyKind::Api).expect("keychain");
    let admin = if keychain::exists("anthropic", KeyKind::Admin) { keychain::get("anthropic", KeyKind::Admin).expect("keychain") } else { None };
    println!("api key present: {} · admin key present: {}", api.is_some(), admin.is_some());
    let owned: Vec<String> = [api.clone(), admin.clone()].into_iter().flatten().collect();
    let secrets: Vec<&str> = owned.iter().map(String::as_str).collect();
    let http = reqwest::Client::builder().timeout(std::time::Duration::from_secs(30)).build().unwrap();
    let ctx = ProviderContext { http, api_key: api, admin_key: admin.clone(), base_url: None };
    let a = adapter(ProviderKind::Anthropic);

    match a.test_connection(&ctx).await {
        Ok(t) => {
            println!(
                "[app test_connection] ok={} · {} · models={} · usage_api={:?}",
                t.ok,
                redact(&t.message, &secrets),
                t.models.len(),
                t.usage_api.map(|u| (u.ok, redact(&u.message, &secrets)))
            );
            println!("  models: {:?}", t.models.iter().take(12).collect::<Vec<_>>());
        }
        Err(e) => println!("[app test_connection] ERROR {}", redact(&e.to_string(), &secrets)),
    }

    if admin.is_some() {
        let now = chrono::Utc::now().timestamp_millis();
        match a.sync_usage(&ctx, now - 30 * 86_400_000, now).await {
            Ok(r) => {
                let tokens: i64 = r.usage.iter().map(|u| u.input_tokens + u.cached_tokens + u.cache_write_tokens + u.output_tokens).sum();
                let cost: f64 = r.costs.iter().map(|c| c.amount_usd).sum();
                println!("[app sync_usage] usage records={} ({tokens} tokens) · cost records={} (${cost:.4})", r.usage.len(), r.costs.len());
                for n in &r.notes {
                    println!("  note: {}", redact(n, &secrets));
                }
            }
            Err(e) => println!("[app sync_usage] ERROR {}", redact(&e.to_string(), &secrets)),
        }
    } else {
        println!("[app sync_usage] skipped: no admin key (official usage needs one)");
    }
}
