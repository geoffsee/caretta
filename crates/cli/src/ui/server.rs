#[cfg(not(target_arch = "wasm32"))]
use crate::agent::workflow::{WorkflowEntry, list_presets, load_sidebar_entries};
#[cfg(not(target_arch = "wasm32"))]
use axum::{
    Json, Router,
    body::Body,
    extract::Path,
    extract::State,
    http::{StatusCode, header},
    response::IntoResponse,
    routing::get,
};
#[cfg(not(target_arch = "wasm32"))]
use rust_embed::RustEmbed;
#[cfg(not(target_arch = "wasm32"))]
use serde::Serialize;
#[cfg(not(target_arch = "wasm32"))]
use std::net::SocketAddr;
#[cfg(not(target_arch = "wasm32"))]
use tracing::info;

#[cfg(not(target_arch = "wasm32"))]
#[derive(RustEmbed)]
#[folder = "$WEB_ASSETS_DIR"]
struct WebAssets;

#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone)]
struct AppState {
    root: String,
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Serialize)]
struct PresetsResponse {
    presets: Vec<String>,
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Serialize)]
struct WorkflowsResponse {
    workflows: Vec<WorkflowEntry>,
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
}

#[cfg(not(target_arch = "wasm32"))]
pub async fn serve(root: String, port: u16) -> anyhow::Result<()> {
    let state = AppState { root };
    let app = Router::new()
        .route("/api/health", get(api_health))
        .route("/api/workflows/presets", get(api_list_presets))
        .route("/api/workflows/:preset", get(api_list_workflows))
        .route("/", get(index_handler))
        .fallback(static_handler)
        .with_state(state);

    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|e| anyhow::anyhow!("failed to bind to {}: {}", addr, e))?;
    let local_addr = listener
        .local_addr()
        .map_err(|e| anyhow::anyhow!("failed to read local addr: {}", e))?;
    info!("Serving web UI on http://{}", local_addr);
    info!("API routes available:");
    info!("  GET /api/health");
    info!("  GET /api/workflows/presets");
    info!("  GET /api/workflows/<preset>");
    axum::serve(listener, app)
        .await
        .map_err(|e| anyhow::anyhow!("axum server error: {}", e))?;
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
async fn api_list_presets(State(state): State<AppState>) -> impl IntoResponse {
    let presets = list_presets(&state.root);
    info!("GET /api/workflows/presets => {} presets", presets.len());
    Json(PresetsResponse { presets })
}

#[cfg(not(target_arch = "wasm32"))]
async fn api_health() -> impl IntoResponse {
    info!("GET /api/health => ok");
    Json(HealthResponse { status: "ok" })
}

#[cfg(not(target_arch = "wasm32"))]
async fn api_list_workflows(
    State(state): State<AppState>,
    Path(preset): Path<String>,
) -> impl IntoResponse {
    let workflows = load_sidebar_entries(&state.root, &preset);
    info!(
        "GET /api/workflows/{} => {} workflows",
        preset,
        workflows.len()
    );
    Json(WorkflowsResponse { workflows })
}

#[cfg(not(target_arch = "wasm32"))]
async fn index_handler() -> impl IntoResponse {
    serve_static_file("index.html").into_response()
}

#[cfg(not(target_arch = "wasm32"))]
async fn static_handler(req: axum::http::Request<Body>) -> impl IntoResponse {
    let path = req.uri().path().trim_start_matches('/');
    let path_to_use = if path.is_empty() { "index.html" } else { path };
    let response = serve_static_file(path_to_use);

    if path_to_use == "index.html" {
        info!(
            "GET /{} -> index fallback",
            req.uri().path().trim_start_matches('/')
        );
    }

    response.into_response()
}

#[cfg(not(target_arch = "wasm32"))]
fn serve_static_file(file_path: &str) -> impl IntoResponse {
    match WebAssets::get(file_path) {
        Some(content) => {
            let mime = mime_guess::from_path(file_path).first_or_octet_stream();
            ([(header::CONTENT_TYPE, mime.as_ref())], content.data).into_response()
        }
        None => {
            let missing_asset_message = "Web UI assets are not bundled in this build.\nRun a web build first:\n  (from repo root) `cd crates/cli && dx build --platform web`\nThen restart `caretta serve`.";
            if file_path != "index.html"
                && let Some(content) = WebAssets::get("index.html")
            {
                let mime = mime_guess::from_path("index.html").first_or_octet_stream();
                ([(header::CONTENT_TYPE, mime.as_ref())], content.data).into_response()
            } else {
                (
                    StatusCode::NOT_FOUND,
                    [(
                        header::CONTENT_TYPE,
                        "text/plain; charset=utf-8",
                    )],
                    missing_asset_message,
                )
                    .into_response()
            }
        }
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;

    #[test]
    fn workflows_api_shape_matches_sidebar_entries() {
        let root = env!("CARGO_MANIFEST_DIR");
        let payload = serde_json::to_value(WorkflowsResponse {
            workflows: load_sidebar_entries(root, "default"),
        })
        .expect("serialize workflows response");

        let workflows = payload
            .get("workflows")
            .and_then(|v| v.as_array())
            .expect("workflows array");

        assert!(
            workflows
                .iter()
                .any(|wf| wf.get("id").and_then(|v| v.as_str()) == Some("ideation")),
            "default preset should expose ideation in web API payload"
        );
        assert!(
            workflows
                .iter()
                .all(|wf| wf.get("category").and_then(|v| v.as_str()).is_some()),
            "web API payload should expose normalized top-level category fields"
        );
    }
}
