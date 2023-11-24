#![feature(never_type)]

use axum::{
    extract::State,
    routing::get,
    Router,
    body::StreamBody,
};

use tower_http::{
    LatencyUnit,
    services::ServeFile,
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

use std::{sync::Arc, path::Path};

struct AppState {
    broadcast: broadcast::Sender<Vec<u8>>,
}

// TODO This can keep the server alive even after `main` has exited. I guess
// we should preferably send some shutdown signal to these tasks to indicate
// that it they stop streaming.
async fn get_data(State(state): State<Arc<AppState>>) -> StreamBody<impl Stream<Item = Result<Vec<u8>, !>>> {
    let broadcast = &state.broadcast;
    let rx = broadcast.subscribe();
    StreamBody::new(tokio_stream::wrappers::BroadcastStream::new(rx).filter_map(|x| async move { x.ok().map(|v| Result::Ok(v)) }))
}

#[tokio::main]
async fn main() {
    let public_dir = "public"; // env::var("PUBLIC_DIR").expect("PUBLIC_DIR must be set");

    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();

    let (tx, _) = broadcast::channel(4);
    let shared_state = Arc::new(AppState {
        broadcast: tx.clone(),
    });

    let app = Router::new()
        .nest_service("/", ServeFile::new(Path::new(public_dir).join("index.html")))
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
            let listener = TcpListener::bind("0.0.0.0:8001").await.unwrap();

            let mut read_buffer = [0u8; 512];
            loop {
                if let Ok((mut socket, _)) = listener.accept().await {
                    println!("Received sender");
                    while let Ok(n) = socket.read(&mut read_buffer).await {
                        // NOTE We're doing a lot of dynamic memory alloaction
                        // here. :(
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
