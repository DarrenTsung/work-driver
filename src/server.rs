use crate::state::{load_state, save_state};
use anyhow::Result;
use axum::http::StatusCode;
use axum::response::Html;
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::Utc;
use serde::Deserialize;
use std::fs;
use tower_http::cors::CorsLayer;

#[derive(Deserialize)]
struct SeenRequest {
    issue: String,
}

async fn index() -> Result<Html<String>, StatusCode> {
    let path = shellexpand::tilde("~/Desktop/work-driver-issues.html");
    let content = fs::read_to_string(path.as_ref()).map_err(|_| StatusCode::NOT_FOUND)?;
    Ok(Html(content))
}

async fn mark_seen(Json(body): Json<SeenRequest>) -> Result<StatusCode, StatusCode> {
    let mut state = load_state().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    state.seen.insert(body.issue, Utc::now());
    save_state(&state).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::OK)
}

async fn get_state() -> Result<Json<serde_json::Value>, StatusCode> {
    let state = load_state().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let value = serde_json::to_value(state).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(value))
}

pub async fn run_server() -> Result<()> {
    let app = Router::new()
        .route("/", get(index))
        .route("/seen", post(mark_seen))
        .route("/state", get(get_state))
        .layer(CorsLayer::permissive());

    let listener = tokio::net::TcpListener::bind("127.0.0.1:9845").await?;
    println!("Server listening on http://127.0.0.1:9845");
    axum::serve(listener, app).await?;
    Ok(())
}
