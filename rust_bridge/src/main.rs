use std::{
    net::SocketAddr,
    process::Stdio,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::{anyhow, Context, Result};
use axum::{
    body::{Body, Bytes},
    extract::State,
    http::{header, HeaderMap, HeaderValue, Method, Request, StatusCode},
    response::{IntoResponse, Response},
    routing::{any, get, post},
    Json, Router,
};
use clap::Parser;
use futures_util::TryStreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tokio::{
    process::{Child, Command},
    signal,
    sync::Mutex,
    time::sleep,
};
use tracing::{error, info, warn};

#[derive(Debug, Parser)]
#[command(name = "lemonade-cake-bridge")]
struct Args {
    #[arg(long, default_value = "127.0.0.1")]
    listen_host: String,

    #[arg(long)]
    listen_port: u16,

    #[arg(long, default_value_t = 18080)]
    cake_port: u16,

    #[arg(long)]
    model: String,

    #[arg(
        long,
        env = "LEMONADE_CAKE_COMMAND",
        default_value = "cake master --model {model} --api 127.0.0.1:{port} --device {device}"
    )]
    cake_command: String,

    /// GPU device index substituted for `{device}` in the Cake command template.
    #[arg(long, env = "LEMONADE_CAKE_DEVICE", default_value_t = 0)]
    cake_device: u32,

    #[arg(long, default_value_t = 600)]
    startup_timeout_seconds: u64,

    #[arg(long, default_value = "info")]
    log_level: String,
}

#[derive(Debug, Clone, Serialize)]
struct BridgeCapabilities {
    bridge_schema: &'static str,
    inference: bool,
    streaming: bool,
    models: bool,
    cake_topology: bool,
    moe_router_visibility: Visibility,
    expert_residency_visibility: Visibility,
    expert_transfer_visibility: Visibility,
    expert_control: ControlCapability,
    placement_plan_validation: bool,
    placement_plan_apply: bool,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
enum Visibility {
    Available,
    Partial,
    Unavailable,
    Unknown,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
enum ControlCapability {
    ShadowOnly,
    Available,
    Unavailable,
}

#[derive(Debug, Clone, Serialize)]
struct RuntimeFingerprint {
    schema: &'static str,
    observed_runtime: &'static str,
    claimed_runtime: &'static str,
    bridge_version: &'static str,
    model: String,
    cake_base_url: String,
    bridge_pid: u32,
    cake_pid: Option<u32>,
    cake_command_sha256: String,
    verified_at_unix_ms: u128,
    cake_models_observed: Value,
    runtime_identity_verified: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PlacementPlan {
    schema: String,
    #[serde(default)]
    run_id: Option<String>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    assignments: Vec<Value>,
    #[serde(default)]
    reason: Option<String>,
}

struct AppState {
    client: Client,
    cake_base_url: String,
    model: String,
    cake_command: String,
    child: Mutex<Option<Child>>,
}

impl AppState {
    async fn cake_pid(&self) -> Option<u32> {
        self.child.lock().await.as_ref().and_then(Child::id)
    }
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

fn render_command(template: &str, model: &str, port: u16, device: u32) -> String {
    template
        .replace("{model}", model)
        .replace("{port}", &port.to_string())
        .replace("{device}", &device.to_string())
}

/// The runtime fingerprint only claims Cake when `/v1/models` carried an
/// OpenAI-style `data` array.
fn models_identify_cake(models: &Value) -> bool {
    models.get("data").map(Value::is_array).unwrap_or(false)
}

fn command_sha256(command: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(command.as_bytes());
    format!("{:x}", hasher.finalize())
}

#[cfg(unix)]
fn command_from_template(rendered: &str) -> Command {
    let mut cmd = Command::new("sh");
    cmd.arg("-lc").arg(rendered);
    cmd
}

#[cfg(windows)]
fn command_from_template(rendered: &str) -> Command {
    let mut cmd = Command::new("cmd");
    cmd.arg("/C").arg(rendered);
    cmd
}

async fn launch_cake(args: &Args) -> Result<Child> {
    let rendered = render_command(
        &args.cake_command,
        &args.model,
        args.cake_port,
        args.cake_device,
    );
    info!(command = %rendered, "launching Cake");

    let mut cmd = command_from_template(&rendered);
    cmd.stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .kill_on_drop(true);

    cmd.spawn().context("failed to launch Cake command")
}

async fn wait_for_cake(
    client: &Client,
    cake_base_url: &str,
    timeout: Duration,
) -> Result<Value> {
    let started = tokio::time::Instant::now();
    let models_url = format!("{cake_base_url}/v1/models");

    loop {
        if started.elapsed() > timeout {
            return Err(anyhow!("Cake did not become ready before timeout"));
        }

        match client.get(&models_url).send().await {
            Ok(response) if response.status().is_success() => {
                let body = response
                    .json::<Value>()
                    .await
                    .context("Cake /v1/models returned invalid JSON")?;
                return Ok(body);
            }
            Ok(response) => {
                warn!(status = %response.status(), "Cake not ready yet");
            }
            Err(_) => {}
        }

        sleep(Duration::from_millis(250)).await;
    }
}

async fn bridge_health(State(state): State<Arc<AppState>>) -> Response {
    let url = format!("{}/v1/models", state.cake_base_url);
    match state.client.get(url).send().await {
        Ok(resp) if resp.status().is_success() => Json(json!({
            "status": "ok",
            "backend": "cake",
            "model": state.model,
            "cake_pid": state.cake_pid().await
        }))
        .into_response(),
        Ok(resp) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({"status": "degraded", "cake_status": resp.status().as_u16()})),
        )
            .into_response(),
        Err(err) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({"status": "down", "error": err.to_string()})),
        )
            .into_response(),
    }
}

async fn capabilities() -> Json<BridgeCapabilities> {
    Json(BridgeCapabilities {
        bridge_schema: "lemonade.cake.capabilities.v1",
        inference: true,
        streaming: true,
        models: true,
        cake_topology: true,
        moe_router_visibility: Visibility::Unknown,
        expert_residency_visibility: Visibility::Unknown,
        expert_transfer_visibility: Visibility::Unknown,
        expert_control: ControlCapability::ShadowOnly,
        placement_plan_validation: true,
        placement_plan_apply: false,
    })
}

async fn runtime_fingerprint(State(state): State<Arc<AppState>>) -> Response {
    let models_url = format!("{}/v1/models", state.cake_base_url);
    let models = match state.client.get(models_url).send().await {
        Ok(resp) if resp.status().is_success() => {
            resp.json::<Value>().await.unwrap_or_else(|_| json!(null))
        }
        _ => json!(null),
    };

    let observed_cake = models_identify_cake(&models);

    Json(RuntimeFingerprint {
        schema: "lemonade.runtime-fingerprint.v1",
        observed_runtime: if observed_cake { "cake" } else { "unknown" },
        claimed_runtime: "cake",
        bridge_version: env!("CARGO_PKG_VERSION"),
        model: state.model.clone(),
        cake_base_url: state.cake_base_url.clone(),
        bridge_pid: std::process::id(),
        cake_pid: state.cake_pid().await,
        cake_command_sha256: command_sha256(&state.cake_command),
        verified_at_unix_ms: now_ms(),
        cake_models_observed: models,
        runtime_identity_verified: observed_cake,
    })
    .into_response()
}

async fn topology(State(state): State<Arc<AppState>>) -> Response {
    proxy_get(&state, "/api/v1/topology").await
}

async fn validate_placement(
    State(state): State<Arc<AppState>>,
    Json(plan): Json<PlacementPlan>,
) -> Response {
    let topology = match state
        .client
        .get(format!("{}/api/v1/topology", state.cake_base_url))
        .send()
        .await
    {
        Ok(resp) if resp.status().is_success() => {
            resp.json::<Value>().await.unwrap_or_else(|_| json!(null))
        }
        _ => json!(null),
    };

    Json(json!({
        "schema": "lemonade.cake.placement-validation.v1",
        "mode": "shadow",
        "accepted_for_execution": false,
        "syntactically_valid": !plan.schema.is_empty(),
        "plan": plan,
        "observed_topology": topology,
        "note": "Placement application is intentionally disabled in v1."
    }))
    .into_response()
}

async fn apply_placement(Json(plan): Json<PlacementPlan>) -> Response {
    (
        StatusCode::NOT_IMPLEMENTED,
        Json(json!({
            "schema": "lemonade.cake.placement-apply.v1",
            "mode": "shadow",
            "applied": false,
            "plan": plan,
            "error": "Cake placement control is not enabled in this integration."
        })),
    )
        .into_response()
}

async fn proxy_get(state: &AppState, path: &str) -> Response {
    let url = format!("{}{}", state.cake_base_url, path);
    match state.client.get(url).send().await {
        Ok(resp) => response_from_reqwest(resp).await,
        Err(err) => gateway_error(err),
    }
}

async fn proxy_any(
    State(state): State<Arc<AppState>>,
    req: Request<Body>,
) -> Response {
    let method = req.method().clone();
    let path = req.uri().path_and_query().map(|x| x.as_str()).unwrap_or("/");
    let url = format!("{}{}", state.cake_base_url, path);
    let headers = req.headers().clone();

    let body = match axum::body::to_bytes(req.into_body(), usize::MAX).await {
        Ok(bytes) => bytes,
        Err(err) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"error": format!("failed to read request body: {err}")})),
            )
                .into_response()
        }
    };

    let mut request = state.client.request(method, url).body(body);
    request = copy_forward_headers(request, &headers);

    match request.send().await {
        Ok(resp) => response_from_reqwest(resp).await,
        Err(err) => gateway_error(err),
    }
}

fn copy_forward_headers(
    mut builder: reqwest::RequestBuilder,
    headers: &HeaderMap,
) -> reqwest::RequestBuilder {
    for (name, value) in headers {
        if name == header::HOST || name == header::CONTENT_LENGTH {
            continue;
        }
        builder = builder.header(name, value);
    }
    builder
}

async fn response_from_reqwest(resp: reqwest::Response) -> Response {
    let status = resp.status();
    let headers = resp.headers().clone();
    let stream = resp
        .bytes_stream()
        .map_err(std::io::Error::other);

    let mut response = Response::new(Body::from_stream(stream));
    *response.status_mut() = status;

    for (name, value) in &headers {
        if name == header::TRANSFER_ENCODING || name == header::CONTENT_LENGTH {
            continue;
        }
        response.headers_mut().insert(name.clone(), value.clone());
    }

    response
}

fn gateway_error(err: reqwest::Error) -> Response {
    (
        StatusCode::BAD_GATEWAY,
        Json(json!({"error": format!("Cake backend request failed: {err}")})),
    )
        .into_response()
}

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c().await.expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_new(&args.log_level)
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let client = Client::builder()
        .connect_timeout(Duration::from_secs(3))
        .build()
        .context("failed to build HTTP client")?;

    let cake_base_url = format!("http://127.0.0.1:{}", args.cake_port);
    let child = launch_cake(&args).await?;

    let models = wait_for_cake(
        &client,
        &cake_base_url,
        Duration::from_secs(args.startup_timeout_seconds),
    )
    .await?;

    info!(models = %models, "Cake runtime identity verified");

    let state = Arc::new(AppState {
        client,
        cake_base_url,
        model: args.model.clone(),
        cake_command: render_command(
            &args.cake_command,
            &args.model,
            args.cake_port,
            args.cake_device,
        ),
        child: Mutex::new(Some(child)),
    });

    let app = Router::new()
        .route("/health", get(bridge_health))
        .route("/internal/capabilities", get(capabilities))
        .route("/internal/runtime-fingerprint", get(runtime_fingerprint))
        .route("/internal/placement/validate", post(validate_placement))
        .route("/internal/placement/apply", post(apply_placement))
        .route("/api/v1/topology", get(topology))
        .route("/v1/models", any(proxy_any))
        .route("/v1/chat/completions", any(proxy_any))
        .fallback(any(proxy_any))
        .with_state(state.clone());

    let addr: SocketAddr = format!("{}:{}", args.listen_host, args.listen_port)
        .parse()
        .context("invalid bridge listen address")?;

    info!(%addr, cake = %state.cake_base_url, "Lemonade Cake bridge ready");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    let server = axum::serve(listener, app).with_graceful_shutdown(shutdown_signal());

    if let Err(err) = server.await {
        error!(%err, "bridge server failed");
    }

    if let Some(mut child) = state.child.lock().await.take() {
        let _ = child.kill().await;
        let _ = child.wait().await;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_command_substitutes_all_placeholders() {
        let out = render_command("cake master --model {model} --api 127.0.0.1:{port} --device {device}", "org/m", 18080, 2);
        assert_eq!(out, "cake master --model org/m --api 127.0.0.1:18080 --device 2");
    }

    #[test]
    fn render_command_replaces_repeated_placeholders_and_keeps_unknown() {
        let out = render_command("{port}-{port} {other}", "m", 9, 0);
        assert_eq!(out, "9-9 {other}");
    }

    #[test]
    fn default_template_matches_cake_0_1_0_cli() {
        let args = Args::parse_from(["bridge", "--listen-port", "1", "--model", "m"]);
        assert!(args.cake_command.starts_with("cake master "));
        assert!(args.cake_command.contains("--api 127.0.0.1:{port}"));
        assert!(!args.cake_command.contains("serve"));
        assert_eq!(args.cake_device, 0);
    }

    #[test]
    fn models_identity_requires_data_array() {
        assert!(models_identify_cake(&json!({"data": []})));
        assert!(models_identify_cake(&json!({"data": [{"id": "x"}]})));
        assert!(!models_identify_cake(&json!({"data": {}})));
        assert!(!models_identify_cake(&json!({"models": []})));
        assert!(!models_identify_cake(&json!(null)));
    }

    #[test]
    fn command_sha256_is_stable_and_input_sensitive() {
        assert_eq!(
            command_sha256("abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_ne!(command_sha256("a"), command_sha256("b"));
    }

    #[test]
    fn placement_plan_defaults_optional_fields() {
        let plan: PlacementPlan = serde_json::from_value(json!({"schema": "s"})).unwrap();
        assert_eq!(plan.schema, "s");
        assert!(plan.assignments.is_empty());
        assert!(plan.run_id.is_none() && plan.model.is_none() && plan.reason.is_none());
        assert!(serde_json::from_value::<PlacementPlan>(json!({"assignments": []})).is_err());
    }

    #[test]
    fn capabilities_are_shadow_only() {
        let v = serde_json::to_value(BridgeCapabilities {
            bridge_schema: "x",
            inference: true,
            streaming: true,
            models: true,
            cake_topology: true,
            moe_router_visibility: Visibility::Unknown,
            expert_residency_visibility: Visibility::Unknown,
            expert_transfer_visibility: Visibility::Unknown,
            expert_control: ControlCapability::ShadowOnly,
            placement_plan_validation: true,
            placement_plan_apply: false,
        })
        .unwrap();
        assert_eq!(v["expert_control"], "shadow_only");
        assert_eq!(v["moe_router_visibility"], "unknown");
    }
}
