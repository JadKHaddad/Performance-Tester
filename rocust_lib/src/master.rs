use crate::{test::Test, LogType, Logger, Results, Status, Updatble};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fmt;
use std::sync::atomic::AtomicU32;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use std::time::Instant;
use tokio::select;
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;

use futures_util::{SinkExt, StreamExt};
use poem::{
    get, handler,
    listener::TcpListener,
    middleware::Tracing,
    web::{
        websocket::{Message, WebSocket},
        Data,
    },
    EndpointExt, IntoResponse, Route, Server,
};

#[derive(Debug, Deserialize, Serialize)]
pub enum WebSocketMessage {
    Create(Test, u32),
    Start,
    Stop,
    Finish,
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
            WebSocketMessage::Finish => write!(f, "Finish"),
        }
    }
}
#[derive(Debug)]
struct State {
    status: RwLock<Status>,
    workers_count: u32,
    connected_workers: AtomicU32,
    test: Test,
    tx: broadcast::Sender<String>,
    logger: Logger,
}

impl State {
    fn increase_connected_workers(&self) {
        self.connected_workers
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }
    fn decrease_connected_workers(&self) {
        self.connected_workers
            .fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
    }
    fn get_connected_workers(&self) -> u32 {
        self.connected_workers
            .load(std::sync::atomic::Ordering::SeqCst)
    }
    pub fn set_status(&self, status: Status) {
        *self.status.write() = status;
    }
}

#[derive(Debug, Clone)]
pub struct Master {
    token: Arc<Mutex<CancellationToken>>,
    addr: String,
    state: Arc<State>,
}

impl Master {
    pub fn new(workers_count: u32, test: Test, addr: String, logfile_path: String) -> Master {
        let (tx, _rx) = broadcast::channel(100);
        let state = Arc::new(State {
            status: RwLock::new(Status::CREATED),
            workers_count,
            connected_workers: AtomicU32::new(0),
            test,
            tx,
            logger: Logger::new(logfile_path),
        });
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
        self.state
            .logger
            .log_buffered(LogType::INFO, &format!("Running on http://{}", self.addr));
        println!("Running on http://{}", self.addr);

        Server::new(TcpListener::bind(self.addr.clone()))
            .run(app)
            .await
    }

    pub async fn run(&self) {
        self.set_status(Status::RUNNING);
        //run background thread
        let master_handle = self.clone();
        let background_join_handle = tokio::spawn(async move {
            master_handle.run_update_in_background(1).await;
        });
        //set run time
        let master_handle = self.clone();
        let mut run_message = String::from("Master running forever, press ctrl+c to stop");
        if let Some(run_time) = master_handle.state.test.get_run_time().clone() {
            run_message = format!("Master running for {} seconds", run_time);

            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_secs(run_time)).await;
                master_handle.finish();
                master_handle
                    .state
                    .logger
                    .log_buffered(LogType::INFO, &format!("Master finished"));
            });
        }
        self.state.logger.log_buffered(LogType::INFO, &run_message);
        let token = self.token.lock().unwrap().clone();
        select! {
            _ = token.cancelled() => {

            }
            _ = self.run_forever() => {
            }
        }
        match background_join_handle.await {
            Ok(_) => {}
            Err(e) => {
                self.state
                    .logger
                    .log_buffered(LogType::ERROR, &format!("{}", e));
            }
        }
        self.state
            .logger
            .log_buffered(LogType::INFO, &format!("Background thread stopped"));
        //flush buffer
        let _ = self.state.logger.flush_buffer().await;
    }

    pub fn stop(&self) {
        self.set_status(Status::STOPPED);
        self.token.lock().unwrap().cancel();
        //on stop tell the workers to stop
        let message = WebSocketMessage::Stop;
        if let Some(json) = message.into_json() {
            if self.state.tx.send(json).is_err() {
                self.state
                    .logger
                    .log_buffered(LogType::ERROR, &format!("Error sending message to worker"));
            }
        }
    }

    fn finish(&self) {
        self.set_status(Status::FINISHED);
        self.token.lock().unwrap().cancel();
        //send finish message to workers
        let message = WebSocketMessage::Finish;
        if let Some(json) = message.into_json() {
            if self.state.tx.send(json).is_err() {
                self.state
                    .logger
                    .log_buffered(LogType::ERROR, &format!("Error sending message to worker"));
            }
        }
    }

    fn set_status(&self, status: Status) {
        self.state.set_status(status);
    }

    async fn run_update_in_background(&self, thread_sleep_time: u64) {
        let token = self.token.lock().unwrap().clone();
        select! {
            _ = token.cancelled() => {

            }
            _ = self.update_in_background(thread_sleep_time) => {

            }
        }
    }

    async fn update_in_background(&self, thread_sleep_time: u64) {
        loop {
            //calculate requests per second
            if let Some(elapsed) = Test::calculate_elapsed_time(
                *self.state.test.get_start_timestamp().read(),
                *self.state.test.get_end_timestamp().read(),
            ) {
                self.state.test.calculate_requests_per_second(&elapsed);
                self.state
                    .test
                    .calculate_failed_requests_per_second(&elapsed);
            }
            //print stats
            //self.state.test.print_stats();
            //log
            let _ = self.state.logger.flush_buffer().await;
            tokio::time::sleep(Duration::from_secs(thread_sleep_time)).await;
        }
    }
}

#[handler]
fn ws(ws: WebSocket, state: Data<&Arc<State>>) -> impl IntoResponse {
    let state = state.clone();
    let state_clone = state.clone();
    let sender = state.tx.clone();
    let mut receiver = sender.subscribe();

    ws.on_upgrade(move |socket| async move {
        let (mut sink, mut stream) = socket.split();
        let test = state.test.clone();
        tokio::spawn(async move {
            state.increase_connected_workers();
            state
                .logger
                .log_buffered(LogType::INFO, &format!("Worker connected"));
            if state.get_connected_workers() == state.workers_count {
                state.logger.log_buffered(
                    LogType::INFO,
                    &format!("All workers connected. Starting test"),
                );
                let message = WebSocketMessage::Start;
                if let Some(json) = message.into_json() {
                    if sender.send(json).is_err() {
                        state.logger.log_buffered(
                            LogType::ERROR,
                            &format!("Error sending message to worker"),
                        );
                        return;
                    }
                }
                state.test.set_start_timestamp(Instant::now());
                //Test will not start here, it will start in the workers
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
                    state_clone
                        .logger
                        .log_buffered(LogType::ERROR, &format!("Error sending message to worker"));
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
