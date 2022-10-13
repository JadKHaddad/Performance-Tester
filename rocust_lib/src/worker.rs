use crate::{master::WebSocketMessage, test::Test, LogType, Logger, Runnable, Status};
use async_trait::async_trait;
use futures_channel::mpsc::UnboundedSender;
use futures_util::{future, pin_mut, StreamExt};
use parking_lot::RwLock;
use std::{
    error::Error,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{select, task::JoinHandle};
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone)]
pub struct Worker {
    id: String,
    status: Arc<RwLock<Status>>,
    test: Arc<RwLock<Option<Test>>>,
    master_addr: String,
    token: Arc<Mutex<CancellationToken>>,
    logger: Arc<Logger>,
    test_join_handle: Arc<RwLock<Option<JoinHandle<()>>>>,
    background_join_handle: Arc<RwLock<Option<JoinHandle<()>>>>,
    tx: Arc<RwLock<Option<UnboundedSender<Message>>>>,
}

impl Worker {
    pub fn new(id: String, master_addr: String, logfile_path: String) -> Worker {
        Worker {
            id,
            status: Arc::new(RwLock::new(Status::CREATED)),
            test: Arc::new(RwLock::new(None)),
            master_addr,
            token: Arc::new(Mutex::new(CancellationToken::new())),
            logger: Arc::new(Logger::new(logfile_path)),
            test_join_handle: Arc::new(RwLock::new(None)),
            background_join_handle: Arc::new(RwLock::new(None)),
            tx: Arc::new(RwLock::new(None)),
        }
    }

    pub async fn run_forever(&mut self) -> Result<(), Box<dyn Error>> {
        let url = url::Url::parse(&self.master_addr)?;
        let (tx, rx) = futures_channel::mpsc::unbounded();
        let (ws_stream, _) = connect_async(url).await?;
        self.set_status(Status::CONNECTED);
        self.logger
            .log_buffered(LogType::INFO, "Connected to master");
        let (write, read) = ws_stream.split();
        self.tx = Arc::new(RwLock::new(Some(tx)));

        let ws_out = rx.map(Ok).forward(write);
        let ws_in = {
            read.for_each(|message| async {
                if let Ok(msg) = message {
                    match msg {
                        Message::Text(text) => {
                            if let Ok(ws_message) = WebSocketMessage::from_json(&text) {
                                match ws_message {
                                    WebSocketMessage::Create(mut test, user_count) => {
                                        test.set_logger(self.logger.clone());
                                        test.set_run_time(None);
                                        self.logger.log_buffered(
                                            LogType::INFO,
                                            &format!("Creating Test with [{}] users", user_count),
                                        );
                                        *self.test.write() = Some(test);
                                    }

                                    WebSocketMessage::Start => {
                                        self.logger.log_buffered(LogType::INFO, "Starting test");
                                        self.run_test();
                                    }

                                    WebSocketMessage::Stop => {
                                        self.logger.log_buffered(LogType::INFO, "Stopping test");
                                        self.stop();
                                    }

                                    WebSocketMessage::Finish => {
                                        self.logger.log_buffered(LogType::INFO, "Finishing test");
                                        self.finish();
                                    }

                                    _ => {}
                                }
                            } else {
                                self.logger.log_buffered(
                                    LogType::ERROR,
                                    &format!("Invalid message: {}", text),
                                );
                            }
                        }
                        Message::Close(_) => {
                            self.logger
                                .log_buffered(LogType::INFO, "Closing connection");
                            self.logger.log_buffered(LogType::INFO, "Stopping test");
                            self.stop();
                        }
                        _ => {}
                    }
                }
            })
        };
        pin_mut!(ws_out, ws_in);
        future::select(ws_out, ws_in).await; // could totally use tokio::select!
        Ok(())
    }

    fn set_status(&self, status: Status) {
        *self.status.write() = status;
    }

    fn setup_update_in_background(&self) {
        let worker_handle = self.clone();
        let background_join_handle = tokio::spawn(async move {
            worker_handle.run_update_in_background(1).await;
        });
        *self.background_join_handle.write() = Some(background_join_handle);
    }

    fn setup_test_run(&self, mut test: Test) {
        let test_handle = tokio::spawn(async move {
            test.run().await;
        });
        *self.test_join_handle.write() = Some(test_handle);
    }

    pub fn run_test(&self) {
        self.set_status(Status::RUNNING);
        let test = self.test.read().clone();
        if let Some(test) = test {
            self.setup_update_in_background();
            self.setup_test_run(test);
        }
    }

    pub fn stop_test(&self) {
        let guard = self.test.read();
        if let Some(test) = guard.as_ref() {
            test.stop();
        }
    }

    pub fn finish_test(&self) {
        let guard = self.test.read();
        if let Some(test) = guard.as_ref() {
            test.finish();
        }
    }
    async fn join_handles(&self) {
        let background_join_handle = self.background_join_handle.write().take();
        let test_handle = self.test_join_handle.write().take(); // very nice from RwLock ;)

        if let Some(background_join_handle) = background_join_handle {
            self.logger
                .log_buffered(LogType::INFO, "Waiting for background thread to terminate");
            match background_join_handle.await {
                Ok(_) => {}
                Err(e) => {
                    println!("Error while joining background thread: {}", e);
                    self.logger.log_buffered(LogType::ERROR, &format!("{}", e));
                }
            }
        }

        if let Some(test_handle) = test_handle {
            self.logger
                .log_buffered(LogType::INFO, "Waiting for test to terminate");
            match test_handle.await {
                Ok(_) => {}
                Err(e) => {
                    println!("Error while joining test: {}", e);
                    self.logger.log_buffered(LogType::ERROR, &format!("{}", e));
                }
            }
        }
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
        let tx = self.tx.read().clone();
        loop {
            if let Some(ref tx) = tx {
                let message = WebSocketMessage::Update(String::from("test"));
                if let Some(json) = message.into_json() {
                    if tx.unbounded_send(Message::text(json)).is_err() {}
                }
            }
            println!("Updating");
            tokio::time::sleep(Duration::from_secs(thread_sleep_time)).await;
        }
    }
}

#[async_trait]
impl Runnable for Worker {
    async fn run(&mut self) {
        let token = self.token.lock().unwrap().clone();
        select! {
            _ = token.cancelled() => {
            }
            _ = self.run_forever() => {
            }
        }
        self.join_handles().await;
        self.logger
            .log_buffered(LogType::INFO, "Terminating... Bye!");
        //flush buffer
        let _ = self.logger.flush_buffer().await;
    }

    fn stop(&self) {
        self.set_status(Status::STOPPED);
        self.stop_test();
        self.token.lock().unwrap().cancel();
    }

    fn finish(&self) {
        self.set_status(Status::FINISHED);
        self.finish_test();
        self.token.lock().unwrap().cancel();
    }

    fn get_status(&self) -> Status {
        let status = &*self.status.read();
        status.clone()
    }

    fn get_id(&self) -> &String {
        &self.id
    }
}
