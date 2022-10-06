use crate::test::Test;
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fmt;
use std::sync::Arc;
use tokio::sync::broadcast;

use futures_util::{SinkExt, StreamExt};
use poem::{
    get, handler,
    listener::TcpListener,
    middleware::Tracing,
    web::{
        websocket::{Message, WebSocket},
        Data
    },
    EndpointExt, IntoResponse, Route, Server,
};

#[derive(Debug, Deserialize, Serialize)]
pub enum WebSocketMessage {
    Create(Test, u32),
    Start,
    Stop,
}

impl WebSocketMessage {
    pub fn from_json(json: &str) -> Result<Self, Box<dyn Error>> {
        let message: Self = serde_json::from_str(json)?;
        Ok(message)
    }

    pub fn into_json(&self) -> Option<String> {
        if let Ok(json) = serde_json::to_string(self) {
            return Some(json);
        }
        None
    }
}

impl fmt::Display for WebSocketMessage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WebSocketMessage::Create(test, user_count) => {
                write!(f, "Create(Test: {} | User count: {})", test, user_count)
            }
            WebSocketMessage::Start => write!(f, "Start"),
            WebSocketMessage::Stop => write!(f, "Stop"),
        }
    }
}

struct AppState {
    test: Test,
    tx: broadcast::Sender<String>,
}

pub struct Master {
    workers_count: u32,
    addr: String,
    state: Arc<AppState>,
}

#[handler]
fn ws(ws: WebSocket, state: Data<&Arc<AppState>>) -> impl IntoResponse {
    println!("New connection");
    let test = state.test.clone();
    let sender = state.tx.clone();
    let mut receiver = sender.subscribe();

    ws.on_upgrade(move |socket| async move {
        let (mut sink, mut stream) = socket.split();

        tokio::spawn(async move {
            while let Some(Ok(msg)) = stream.next().await {
                if let Message::Text(text) = msg {
                    if sender.send(format!("{}", text)).is_err() {
                        break;
                    }
                }
            }
        });

        tokio::spawn(async move {
            // send the test and the user count to the worker, when the worker is connected
            // TODO: calculate user count
            let message = WebSocketMessage::Create(test, 1);
            if let Some(json) = message.into_json() {
                if sink.send(Message::Text(json)).await.is_err() {
                    println!("Error sending message to worker");
                    return;
                }

                // TODO remove
                // dev
                {
                    let message = WebSocketMessage::Start;
                    if let Some(json) = message.into_json() {
                        if sink.send(Message::Text(json)).await.is_err() {
                            println!("Error sending message to worker");
                            return;
                        }
                    }

                    tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                    let message = WebSocketMessage::Stop;
                    if let Some(json) = message.into_json() {
                        if sink.send(Message::Text(json)).await.is_err() {
                            println!("Error sending message to worker");
                            return;
                        }
                    }

                    //close connection
                    if sink.send(Message::Close(None)).await.is_err() {
                        println!("Error sending message to worker");
                        return;
                    }
                }
            }

            while let Ok(msg) = receiver.recv().await {
                if sink.send(Message::Text(msg)).await.is_err() {
                    break;
                }
            }
        });
    })
}

impl Master {
    pub fn new(workers_count: u32, test: Test, addr: String) -> Master {
        let (tx, _rx) = broadcast::channel(100);
        let state = Arc::new(AppState { test, tx });
        Master {
            workers_count,
            addr,
            state,
        }
    }

    // the master will wait for the workers to connect. then he will send them the test to run and tell each one of them how many users to run.
    // the workers will run the test and send the results back to the master.
    // the master will aggregate the results.
    pub async fn run_forever(&self) -> Result<(), std::io::Error> {
        if std::env::var_os("RUST_LOG").is_none() {
            std::env::set_var("RUST_LOG", "poem=debug");
        }
        tracing_subscriber::fmt::init();

        let app = Route::new()
            .at("/ws", get(ws.data(self.state.clone())))
            .with(Tracing);
        println!("Running on http://{}", self.addr);

        Server::new(TcpListener::bind(self.addr.clone()))
            .run(app)
            .await
    }

    pub fn run(&mut self) {
        todo!()
    }

    pub fn stop(&mut self) {
        todo!()
    }
}
