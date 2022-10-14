use crate::{HasResults, LogType, Logger, Runnable, Status, Test};
use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use parking_lot::RwLock;
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
use serde::{Deserialize, Serialize};
use std::{
    error::Error,
    fmt,
    sync::{
        atomic::{AtomicU32, Ordering::SeqCst},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
use tokio::{
    select,
    sync::{broadcast, mpsc},
    task::JoinHandle,
};
use tokio_util::sync::CancellationToken;

#[derive(Debug, Deserialize, Serialize)]
pub enum WebSocketMessage {
    Create(Test),
    Start,
    Stop,
    Finish,
    Update(String), //TODO: change
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
            WebSocketMessage::Create(test) => {
                write!(f, "Create(Test: {})", test)
            }
            WebSocketMessage::Start => write!(f, "Start"),
            WebSocketMessage::Stop => write!(f, "Stop"),
            WebSocketMessage::Finish => write!(f, "Finish"),
            WebSocketMessage::Update(_) => write!(f, "Update"),
        }
    }
}

#[derive(Debug)]
struct State {
    status: RwLock<Status>,
    workers_count: u32,
    connected_workers: AtomicU32,
    test: Test,
    broadcast_tx: broadcast::Sender<String>,
    logger: Logger,
    background_join_handle: RwLock<Option<JoinHandle<()>>>,
    mpsc_tx: mpsc::Sender<bool>,
    master_cancel_token: CancellationToken,
    remaining_users: AtomicU32,
}

impl State {
    fn get_remaining_users_count(&self) -> u32 {
        self.remaining_users.load(SeqCst)
    }
    fn set_remaining_users_count(&self, value: u32) {
        self.remaining_users.store(value, SeqCst);
    }
    fn increase_connected_workers_count(&self) {
        self.connected_workers.fetch_add(1, SeqCst);
    }
    fn decrease_connected_workers_count(&self) {
        self.connected_workers.fetch_sub(1, SeqCst);
    }
    fn get_connected_workers_count(&self) -> u32 {
        self.connected_workers.load(SeqCst)
    }
    pub fn set_status(&self, status: Status) {
        *self.status.write() = status;
    }
    fn terminate(&self) {
        self.master_cancel_token.cancel();
    }
    fn get_workers_count(&self) -> u32 {
        self.workers_count
    }
}

#[derive(Debug, Clone)]
pub struct Master {
    id: String,
    token: Arc<Mutex<CancellationToken>>,
    addr: String,
    state: Arc<State>,
    mpsc_rx: Arc<RwLock<Option<mpsc::Receiver<bool>>>>,
    print_stats_to_console: Arc<bool>,
}

impl Master {
    pub fn new(
        id: String,
        workers_count: u32,
        test: Test,
        addr: String,
        logfile_path: String,
        print_log_to_console: bool,
        print_stats_to_console: bool,
    ) -> Master {
        let (broadcast_tx, _) = broadcast::channel(100);
        let (mpsc_tx, mpsc_rx) = mpsc::channel::<bool>(1);
        let cancelation_token = CancellationToken::new();
        let user_count = test.get_user_count();
        let workers_count = if workers_count > user_count {
            user_count
        } else {
            workers_count
        };
        let state = Arc::new(State {
            status: RwLock::new(Status::Created),
            workers_count,
            connected_workers: AtomicU32::new(0),
            test,
            broadcast_tx,
            logger: Logger::new(logfile_path, print_log_to_console),
            background_join_handle: RwLock::new(None),
            mpsc_tx,
            master_cancel_token: cancelation_token.clone(),
            remaining_users: AtomicU32::new(user_count),
        });
        Master {
            id,
            token: Arc::new(Mutex::new(cancelation_token)),
            addr,
            state,
            mpsc_rx: Arc::new(RwLock::new(Some(mpsc_rx))),
            print_stats_to_console: Arc::new(print_stats_to_console),
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
            .log_buffered(LogType::Info, &format!("Running on http://{}", self.addr));
        self.state
            .logger
            .log_buffered(LogType::Info, "Waiting for workers to connect");
        println!("Running on http://{}", self.addr);

        Server::new(TcpListener::bind(self.addr.clone()))
            .run(app)
            .await
    }

    fn set_up_run_message(&self) {
        let master_handle = self.clone();
        let mut run_message = String::from("Test running forever, press ctrl+c to stop");
        if let Some(run_time) = master_handle.state.test.get_run_time().clone() {
            run_message = format!("Test running for {} seconds", run_time);
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_secs(run_time)).await;
                master_handle.finish();
                master_handle
                    .state
                    .logger
                    .log_buffered(LogType::Info, &format!("Master finished"));
            });
        }
        self.state.logger.log_buffered(LogType::Info, &run_message);
    }

    fn setup_update_in_background(&self) {
        //run background thread
        let master_handle = self.clone();
        let background_join_handle = tokio::spawn(async move {
            master_handle.run_update_in_background(1).await;
        });
        *self.state.background_join_handle.write() = Some(background_join_handle);
    }

    async fn join_handles(&self) {
        let background_join_handle = self.state.background_join_handle.write().take();
        if let Some(background_join_handle) = background_join_handle {
            self.state
                .logger
                .log_buffered(LogType::Info, "Waiting for background thread to terminate");
            match background_join_handle.await {
                Ok(_) => {}
                Err(e) => {
                    println!("Error while joining background thread: {}", e);
                    self.state
                        .logger
                        .log_buffered(LogType::Error, &format!("{}", e));
                }
            }
        }
    }

    fn run_background_tasks_on_test_start(&self) {
        let master_handle = self.clone();
        let mpsc_rx = self.mpsc_rx.write().take();
        tokio::spawn(async move {
            if let Some(mut mpsc_rx) = mpsc_rx {
                while let Some(_) = mpsc_rx.recv().await {
                    master_handle.set_up_run_message();
                    master_handle.setup_update_in_background();
                    break;
                }
            }
        });
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
            if *self.print_stats_to_console {
                self.state.test.print_stats();
            }
            //log
            let _ = self.state.logger.flush_buffer().await;
            tokio::time::sleep(Duration::from_secs(thread_sleep_time)).await;
        }
    }
}

#[async_trait]
impl Runnable for Master {
    async fn run(&mut self) {
        self.set_status(Status::Running);
        self.run_background_tasks_on_test_start();
        let token = self.token.lock().unwrap().clone();
        select! {
            _ = token.cancelled() => {
            }
            _ = self.run_forever() => {
            }
        }
        self.join_handles().await;
        self.state
            .logger
            .log_buffered(LogType::Info, "Terminating... Bye!");
        //flush buffer
        let _ = self.state.logger.flush_buffer().await;
    }

    fn stop(&self) {
        self.set_status(Status::Stopped);
        self.token.lock().unwrap().cancel();
        //on stop tell the workers to stop
        let message = WebSocketMessage::Stop;
        if let Some(json) = message.into_json() {
            if self.state.broadcast_tx.send(json).is_err() {
                self.state
                    .logger
                    .log_buffered(LogType::Error, &format!("Error sending message to worker"));
            }
        }
    }

    fn finish(&self) {
        self.set_status(Status::Finished);
        self.token.lock().unwrap().cancel();
        //send finish message to workers
        let message = WebSocketMessage::Finish;
        if let Some(json) = message.into_json() {
            if self.state.broadcast_tx.send(json).is_err() {
                self.state
                    .logger
                    .log_buffered(LogType::Error, &format!("Error sending message to worker"));
            }
        }
    }

    fn get_status(&self) -> Status {
        let status = &*self.state.status.read();
        status.clone()
    }

    fn get_id(&self) -> &String {
        &self.id
    }
}

#[handler]
fn ws(ws: WebSocket, state: Data<&Arc<State>>) -> impl IntoResponse {
    let state = state.clone();
    let state_clone = state.clone();
    let sender = state.broadcast_tx.clone();
    let mut receiver = sender.subscribe();

    ws.on_upgrade(move |socket| async move {
        let (mut sink, mut stream) = socket.split();
        let mut test = state.test.clone();
        state.increase_connected_workers_count();
        // TODO: Fix user count
        let remaining_users_count = state.get_remaining_users_count();
        let mut user_count = test.get_user_count() / state.get_workers_count();
        println!("{}", user_count);
        if remaining_users_count < user_count {
            user_count = remaining_users_count;
        }
        state.set_remaining_users_count(remaining_users_count - user_count);
        test.set_user_count(user_count);
        // TODO: Fix user count
        tokio::spawn(async move {
            state.logger.log_buffered(LogType::Info, "Worker connected");
            if state.get_connected_workers_count() == state.workers_count {
                state
                    .logger
                    .log_buffered(LogType::Info, "All workers connected. Starting test");

                if state.mpsc_tx.send(true).await.is_err() {
                    // this is critical, if it fails, the test will not start, so lets just terminate
                    state.logger.log_buffered(
                        LogType::Error,
                        "Error sending message to main thread, test will not start",
                    );
                    // logger will be flushed on end of run method
                    state.terminate();
                    return;
                }
                state.test.set_start_timestamp(Instant::now());
                //Test will not start here, it will start in the workers
                let message = WebSocketMessage::Start;
                if let Some(json) = message.into_json() {
                    if sender.send(json).is_err() {
                        state
                            .logger
                            .log_buffered(LogType::Error, "Error sending message to worker, results will not be correct, a worker might have disconnected");
                        return;
                    }
                }
            }
            while let Some(Ok(msg)) = stream.next().await {
                if let Message::Text(text) = msg {
                    if let Ok(ws_message) = WebSocketMessage::from_json(&text) {
                        match ws_message {
                            WebSocketMessage::Update(s) => {
                                println!("{}", s);
                            }
                            _ => {}
                        }
                    } else {
                        state
                            .logger
                            .log_buffered(LogType::Error, &format!("Invalid message: {}", text));
                    }
                    // if sender.send(format!("{}", text)).is_err() {
                    //     break;
                    // }
                }
            }
            state.decrease_connected_workers_count();
        });

        tokio::spawn(async move {
            // send the test and the user count to the worker, when the worker is connected
            let message = WebSocketMessage::Create(test);
            if let Some(json) = message.into_json() {
                if sink.send(Message::Text(json)).await.is_err() {
                    state_clone
                        .logger
                        .log_buffered(LogType::Error, "Error sending message to worker");
                    return;
                }
                state_clone
                .logger
                .log_buffered(LogType::Info, &format!("Test sent to worker with [{}] users", user_count));
            }
            while let Ok(msg) = receiver.recv().await {
                if sink.send(Message::Text(msg)).await.is_err() {
                    break;
                }
            }
        });
    })
}
