#![feature(never_type)]

use axum::{
    extract::State,
    routing::get,
    Router,
    body::StreamBody,
};

use tower_http::{
    LatencyUnit,
    services::{
        ServeDir,
        // ServeFile,
    },
    trace::{
        TraceLayer,
        DefaultOnResponse,
    },
};

use futures::prelude::*;

use tokio::net::TcpListener;

use tokio::signal;
use tokio::sync::broadcast;
use tokio::io::AsyncReadExt;

use std::sync::Arc;

use std::env;

struct AppState {
    bc: broadcast::Sender<Vec<u8>>,
}

// TODO This can keep the server alive even after `main` has exited. I guess
// we should preferably send some shutdown signal to the channel to indicate
// that it should stop streaming.
async fn get_data(State(state): State<Arc<AppState>>) -> StreamBody<impl Stream<Item = Result<Vec<u8>, !>>> {
    let bc = &state.bc;
    let rx = bc.subscribe();
    StreamBody::new(tokio_stream::wrappers::BroadcastStream::new(rx).filter_map(|x| async move { x.ok().map(|v| Result::Ok(v)) }))
}

#[tokio::main]
async fn main() {
    let (tx, _) = broadcast::channel(4);

    let public_dir = env::var("PUBLIC_DIR").expect("PUBLIC_DIR must be set");

    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();

    let shared_state = Arc::new(AppState {
        bc: tx.clone(),
    });

    let app = Router::new()
        .nest_service("/files", ServeDir::new(public_dir))
        .route("/data", get(get_data))
        .layer(TraceLayer::new_for_http()
            .on_response(DefaultOnResponse::new()
                .level(tracing::Level::INFO)
                .latency_unit(LatencyUnit::Millis))
        )
        .with_state(shared_state);

    let server = axum::Server::bind(&"0.0.0.0:8000".parse().unwrap())
        .serve(app.into_make_service())
        .with_graceful_shutdown(shutdown_signal());

    let sender = async {
        loop {
            let listener = TcpListener::bind("0.0.0.0:8080").await.unwrap();

            let mut read_buffer = [0u8; 512];
            loop {
                if let Ok((mut socket, _)) = listener.accept().await {
                    println!("Received sender");
                    while let Ok(n) = socket.read(&mut read_buffer).await {
                        let _ = tx.send(read_buffer[..n].to_owned());
                    }
                }
            }
        }
    };
    // `sender` runs forever, so `select` is used just to exit everything when
    // `server` returns.
    tokio::select!{
        _ = sender => {}
        _ = server => {}
    };
    println!("Leaving main");
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
    ctrl_c.await;

    #[cfg(unix)]
    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    println!("signal received, starting graceful shutdown");
}
