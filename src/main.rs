use axum::{
    extract::State,
    routing::get,
    Router,
};

use tower_http::{
    LatencyUnit,
    services::{
        ServeDir,
        ServeFile,
    },
    trace::{
        TraceLayer,
        DefaultOnResponse,
    },
};

use tokio::signal;

use std::sync::{Arc, Mutex};

use std::env;

struct AppState {
}

#[tokio::main]
async fn main() {
    let public_dir = env::var("PUBLIC_DIR").expect("PUBLIC_DIR must be set");

    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();

    let shared_state = Arc::new(Mutex::new(AppState {
        db_connection: establish_sqlite_connection()
    }));

    let app = Router::new()
        .nest_service("/", ServeDir::new(public_dir))
        .layer(TraceLayer::new_for_http()
            .on_response(DefaultOnResponse::new()
                .level(tracing::Level::INFO)
                .latency_unit(LatencyUnit::Millis))
        )
        .with_state(shared_state);

    axum::Server::bind(&"0.0.0.0:8000".parse().unwrap())
        .serve(app.into_make_service())
        .with_graceful_shutdown(shutdown_signal())
        .await
        .unwrap();
}

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    // let terminate = std::future::pending::<()>();
    ctrl_c.await;

    #[cfg(unix)]
    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    println!("signal received, starting graceful shutdown");
}
