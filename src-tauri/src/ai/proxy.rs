//! Local telemetry proxy (127.0.0.1 only).
//!
//! Point an SDK's base URL at `http://127.0.0.1:<port>/<provider>/…` and the
//! request is forwarded unchanged to the provider. The response is streamed
//! back untouched while its `usage` metadata is parsed and stored as
//! telemetry. **Request/response content is never stored or logged**, and the
//! proxy never injects credentials: clients send their own key as usual.

use super::provider::{ProviderContext, ProviderKind};
use super::usage::UsageAccumulator;
use super::{adapter, AiService};
use axum::body::{Body, Bytes};
use axum::extract::{Path, Request, State};
use axum::http::{HeaderMap, HeaderName, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::any;
use axum::Router;
use futures_util::StreamExt;
use std::net::SocketAddr;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

const MAX_REQUEST_BODY: usize = 64 * 1024 * 1024;

pub fn client_base_url(kind: ProviderKind, port: u16) -> String {
    let root = format!("http://127.0.0.1:{port}/{}", kind.id());
    match kind {
        ProviderKind::Openai | ProviderKind::Openrouter | ProviderKind::Groq => format!("{root}/v1"),
        _ => root,
    }
}

#[derive(Clone)]
struct ProxyState {
    ai: AiService,
    http: reqwest::Client,
}

pub struct ProxyHandle {
    pub port: u16,
    cancel: CancellationToken,
}

impl ProxyHandle {
    pub fn stop(&self) {
        self.cancel.cancel();
    }
}

pub async fn start(ai: AiService, port: u16, parent: &CancellationToken) -> Result<ProxyHandle, String> {
    let http = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(15))
        .read_timeout(Duration::from_secs(600))
        .build()
        .map_err(|e| e.to_string())?;
    let app = Router::new()
        .route("/{provider}/{*rest}", any(forward))
        .route("/", any(|| async { "Mac Observer telemetry proxy" }))
        .with_state(ProxyState { ai, http });
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|e| format!("cannot listen on 127.0.0.1:{port}: {e}"))?;
    let cancel = parent.child_token();
    let c2 = cancel.clone();
    tokio::spawn(async move {
        tracing::info!(port, "telemetry proxy listening");
        if let Err(e) = axum::serve(listener, app).with_graceful_shutdown(async move { c2.cancelled().await }).await {
            tracing::error!("proxy server error: {e}");
        }
        tracing::info!("telemetry proxy stopped");
    });
    Ok(ProxyHandle { port, cancel })
}

fn is_hop_header(name: &str) -> bool {
    matches!(
        name,
        "host" | "connection" | "keep-alive" | "proxy-authenticate" | "proxy-authorization" | "te" | "trailer"
            | "transfer-encoding" | "upgrade" | "content-length" | "accept-encoding"
    )
}

/// Model from the request: JSON `model`, or Gemini's `models/<id>:method` path.
pub fn request_model(rest: &str, body: &[u8]) -> Option<String> {
    if let Ok(v) = serde_json::from_slice::<serde_json::Value>(body) {
        if let Some(m) = v.get("model").and_then(|m| m.as_str()) {
            return Some(m.to_string());
        }
    }
    let idx = rest.find("models/")?;
    let tail = &rest[idx + 7..];
    let end = tail.find([':', '/', '?']).unwrap_or(tail.len());
    Some(tail[..end].to_string()).filter(|s| !s.is_empty())
}

fn error(status: StatusCode, msg: &str) -> Response {
    (status, axum::Json(serde_json::json!({ "error": { "message": msg, "type": "mac_observer_proxy" } }))).into_response()
}

async fn forward(State(st): State<ProxyState>, Path((provider, rest)): Path<(String, String)>, req: Request) -> Response {
    // DNS-rebinding guard: only accept requests addressed to localhost.
    let host_ok = req
        .headers()
        .get("host")
        .and_then(|h| h.to_str().ok())
        .map(|h| h.starts_with("127.0.0.1") || h.starts_with("localhost") || h.starts_with("[::1]"))
        .unwrap_or(false);
    if !host_ok {
        return error(StatusCode::FORBIDDEN, "proxy only accepts requests to 127.0.0.1");
    }
    let Ok(kind) = ProviderKind::parse(&provider) else {
        return error(StatusCode::NOT_FOUND, "unknown provider prefix");
    };
    let base = match kind {
        ProviderKind::Custom => match st.ai.context(kind).await {
            Ok(ctx) => adapter(kind).upstream_base(&ctx),
            Err(e) => Err(e),
        },
        _ => adapter(kind).upstream_base(&ProviderContext { http: st.http.clone(), api_key: None, admin_key: None, base_url: None }),
    };
    let base = match base {
        Ok(b) => b,
        Err(e) => return error(StatusCode::BAD_GATEWAY, &e.to_string()),
    };
    let query = req.uri().query().map(|q| format!("?{q}")).unwrap_or_default();
    let url = format!("{}/{}{}", base.trim_end_matches('/'), rest.trim_start_matches('/'), query);
    let method = req.method().clone();
    let headers = req.headers().clone();
    let body: Bytes = match axum::body::to_bytes(req.into_body(), MAX_REQUEST_BODY).await {
        Ok(b) => b,
        Err(_) => return error(StatusCode::PAYLOAD_TOO_LARGE, "request body too large"),
    };
    let model_hint = request_model(&rest, &body);

    let mut up = st.http.request(method, &url);
    for (k, v) in headers.iter() {
        if !is_hop_header(k.as_str()) {
            up = up.header(k, v);
        }
    }
    // Ask for an uncompressed body so usage can be parsed while streaming.
    up = up.header("accept-encoding", "identity");
    let resp = match up.body(body).send().await {
        Ok(r) => r,
        Err(e) => return error(StatusCode::BAD_GATEWAY, &format!("upstream request failed: {}", e.without_url())),
    };

    let status = resp.status();
    let mut out_headers = HeaderMap::new();
    for (k, v) in resp.headers().iter() {
        if !is_hop_header(k.as_str()) {
            if let (Ok(n), Ok(val)) = (HeaderName::from_bytes(k.as_str().as_bytes()), HeaderValue::from_bytes(v.as_bytes())) {
                out_headers.append(n, val);
            }
        }
    }
    let content_type = resp.headers().get("content-type").and_then(|v| v.to_str().ok()).unwrap_or("").to_string();
    let record = status.is_success();

    let (tx, rx) = tokio::sync::mpsc::channel::<Result<Bytes, std::io::Error>>(32);
    let ai = st.ai.clone();
    tokio::spawn(async move {
        let mut acc = UsageAccumulator::new(kind.id(), &content_type);
        let mut stream = resp.bytes_stream();
        let mut client_gone = false;
        while let Some(chunk) = stream.next().await {
            match chunk {
                Ok(b) => {
                    if record {
                        acc.feed(&b);
                    }
                    if !client_gone && tx.send(Ok(b)).await.is_err() {
                        client_gone = true; // keep reading to capture final usage
                    }
                }
                Err(e) => {
                    let _ = tx.send(Err(std::io::Error::other(e.without_url().to_string()))).await;
                    return;
                }
            }
        }
        drop(tx);
        if record {
            if let Some(u) = acc.finish() {
                if let Err(e) = ai.record_telemetry(kind.id(), u, model_hint).await {
                    tracing::error!("failed to record proxy telemetry: {e}");
                }
            }
        }
    });

    let body_stream = futures_util::stream::unfold(rx, |mut rx| async move { rx.recv().await.map(|c| (c, rx)) });
    let mut response = Response::new(Body::from_stream(body_stream));
    *response.status_mut() = StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
    *response.headers_mut() = out_headers;
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_from_body_or_path() {
        assert_eq!(request_model("v1/chat/completions", br#"{"model":"gpt-4o"}"#).as_deref(), Some("gpt-4o"));
        assert_eq!(
            request_model("v1beta/models/gemini-2.5-flash:streamGenerateContent", b"{}").as_deref(),
            Some("gemini-2.5-flash")
        );
        assert_eq!(request_model("v1/embeddings", b"not json"), None);
    }

    #[test]
    fn client_urls() {
        assert_eq!(client_base_url(ProviderKind::Openai, 47821), "http://127.0.0.1:47821/openai/v1");
        assert_eq!(client_base_url(ProviderKind::Anthropic, 47821), "http://127.0.0.1:47821/anthropic");
    }
}
