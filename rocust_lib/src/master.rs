use axum::{
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, get_service},
    Router, TypedHeader,
};
use std::{net::SocketAddr, path::PathBuf};
use tower_http::{
    services::ServeDir,
    trace::{DefaultMakeSpan, TraceLayer},
};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use crate::test::Test;

pub struct Master {
    workers_count: u32,
    test: Test,
    host: [u8; 4],
    port: u16,
}

impl Master {
    pub fn new(workers_count: u32, test: Test, host: [u8; 4], port: u16) -> Master {
        Master {
            workers_count,
            test,
            host,
            port,
        }
    }

    // the master will wait for the workers to connect. then he will send them the test to run and tell each one of them how many users to run.
    // the workers will run the test and send the results back to the master.
    // the master will aggregate the results.
    pub async fn run_forever(&self) {
        async fn ws_handler(
            ws: WebSocketUpgrade,
            user_agent: Option<TypedHeader<headers::UserAgent>>,
        ) -> impl IntoResponse {
            if let Some(TypedHeader(user_agent)) = user_agent {
                println!("`{}` connected", user_agent.as_str());
            }

            ws.on_upgrade(handle_socket)
        }

        async fn handle_socket(mut socket: WebSocket) {
            if socket
                .send(Message::Text(String::from("Hi!")))
                .await
                .is_err()
            {
                println!("client disconnected");
                return;
            }
            loop {
                if let Some(msg) = socket.recv().await {
                    if let Ok(msg) = msg {
                        match msg {
                            Message::Text(t) => {
                                println!("client sent str: {:?}", t);
                            }
                            Message::Binary(_) => {
                                println!("client sent binary data");
                            }
                            Message::Ping(_) => {
                                println!("socket ping");
                            }
                            Message::Pong(_) => {
                                println!("socket pong");
                            }
                            Message::Close(_) => {
                                println!("client disconnected");
                                return;
                            }
                        }
                    } else {
                        println!("client disconnected");
                        return;
                    }
                }
            }

            // loop {
            //     println!("saying hi");
            //     if socket
            //         .send(Message::Text(String::from("Hi!")))
            //         .await
            //         .is_err()
            //     {
            //         println!("client disconnected");
            //         return;
            //     }
            //     tokio::time::sleep(std::time::Duration::from_secs(3)).await;
            // }
        }

        tracing_subscriber::registry()
            .with(tracing_subscriber::EnvFilter::new(
                std::env::var("RUST_LOG")
                    .unwrap_or_else(|_| "example_websockets=debug,tower_http=debug".into()),
            ))
            .with(tracing_subscriber::fmt::layer())
            .init();

        let app = Router::new().route("/ws", get(ws_handler)).layer(
            TraceLayer::new_for_http()
                .make_span_with(DefaultMakeSpan::default().include_headers(true)),
        );

        // run it with hyper
        let addr = SocketAddr::from((self.host, self.port));
        tracing::debug!("listening on {}", addr);
        axum::Server::bind(&addr)
            .serve(app.into_make_service())
            .await
            .unwrap();
    }

    pub fn run(&mut self) {
        todo!()
    }

    pub fn stop(&mut self) {
        todo!()
    }
}
