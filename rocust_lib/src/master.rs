use crate::test::Test;
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fmt;
use std::sync::{Arc, Mutex};
use tokio::sync::broadcast;
use std::sync::atomic::AtomicU32;
use tokio_util::sync::CancellationToken;
use tokio::select;

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
#[derive(Debug)]
struct State {
    workers_count: u32,
    connected_workers: AtomicU32,
    test: Test,
    tx: broadcast::Sender<String>,
}

impl State{
    fn increase_connected_workers(&self) {
        self.connected_workers.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }
    fn decrease_connected_workers(&self) {
        self.connected_workers.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
    }
    fn get_connected_workers(&self) -> u32 {
        self.connected_workers.load(std::sync::atomic::Ordering::SeqCst)
    }
}

#[derive(Debug, Clone)]
pub struct Master {
    token: Arc<Mutex<CancellationToken>>,
    addr: String,
    state: Arc<State>,
}


impl Master {
    pub fn new(workers_count: u32, test: Test, addr: String) -> Master {
        let (tx, _rx) = broadcast::channel(100);
        let state = Arc::new(State { workers_count, connected_workers: AtomicU32::new(0), test, tx });
        Master {
            token: Arc::new(Mutex::new(CancellationToken::new())),
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

    pub async fn run(&self) {
        let token = self.token.lock().unwrap().clone();
        select! {
            _ = token.cancelled() => {

            }
            _ = self.run_forever() => {
            }
        }
    }

    pub fn stop(&self) {
        self.token.lock().unwrap().cancel();
        //on stop tell the workers to stop
        let message = WebSocketMessage::Stop;
        if let Some(json) = message.into_json() {
            if self.state.tx.send(json).is_err() {
                println!("Error sending message to worker");
            }
        }
    }
}

#[handler]
fn ws(ws: WebSocket, state: Data<&Arc<State>>) -> impl IntoResponse {
    let state = state.clone();
    let sender = state.tx.clone();
    let mut receiver = sender.subscribe();

    ws.on_upgrade(move |socket| async move {
        let (mut sink, mut stream) = socket.split();
        let test = state.test.clone();
        tokio::spawn(async move {
            state.increase_connected_workers();
            println!("Worker connected");
            if state.get_connected_workers() == state.workers_count {
                println!("All workers connected. Starting test");
                let message = WebSocketMessage::Start;
                if let Some(json) = message.into_json() {
                    if sender.send(json).is_err() {
                        println!("Error sending message to worker");
                        return;
                    }
                }
            }
            while let Some(Ok(msg)) = stream.next().await {
                if let Message::Text(text) = msg {
                    if sender.send(format!("{}", text)).is_err() {
                        break;
                    }
                }
            }
            state.decrease_connected_workers();
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
            }
            while let Ok(msg) = receiver.recv().await {
                if sink.send(Message::Text(msg)).await.is_err() {
                    break;
                }
            }
        });
    })
}
