use crate::{
    master::{ControlWebSocketMessage, ResultsWebsocketMessage, WS_ENDPOINT},
    traits::HasResults,
    LogType, Logger, Runnable, Status, Test
};
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
    print_stats_to_console: bool,
}

impl Worker {
    pub fn new(
        id: String,
        master_addr: String,
        logfile_path: String,
        print_log_to_console: bool,
        print_stats_to_console: bool,
    ) -> Worker {
        Worker {
            id,
            status: Arc::new(RwLock::new(Status::Created)),
            test: Arc::new(RwLock::new(None)),
            master_addr,
            token: Arc::new(Mutex::new(CancellationToken::new())),
            logger: Arc::new(Logger::new(logfile_path, print_log_to_console)),
            test_join_handle: Arc::new(RwLock::new(None)),
            background_join_handle: Arc::new(RwLock::new(None)),
            tx: Arc::new(RwLock::new(None)),
            print_stats_to_console,
        }
    }

    fn create_results_websocket_message(&self) -> Option<ResultsWebsocketMessage> {
        if let Some(ref test) = *self.test.read() {
            let agg_sent_results = test.clone_results().create_sent_results();
            let endpoints_sent_results = test.create_endpoints_sent_results();
            let results_websocket_message = ResultsWebsocketMessage::new(agg_sent_results, endpoints_sent_results);
            Some(results_websocket_message)
        } else {
            None
        }
    }

    async fn parse_url(&self) -> Result<url::Url, Box<dyn Error>> {
        let url = match url::Url::parse(&self.master_addr) {
            Ok(url) => url,
            Err(e) => {
                return Err(e.into());
            }
        };
        let ws_scheme: &str;
        let host: String;
        let port: u16;
        let origin = url.origin();
        match origin {
            url::Origin::Tuple(scheme, _host, _port) => {
                if scheme == String::from("http") {
                    ws_scheme = "ws";
                } else if scheme == String::from("https") {
                    ws_scheme = "wss";
                } else {
                    let message = format!("Unknown scheme: {}", scheme);
                    return Err(message.into());
                }
                match _host {
                    url::Host::Domain(d) => {
                        host = d;
                    }
                    url::Host::Ipv4(i) => {
                        host = i.to_string();
                    }
                    url::Host::Ipv6(i) => {
                        host = i.to_string();
                    }
                }
                port = _port;
            }
            _ => {
                let message = format!("Invalid url: {}", url);
                return Err(message.into());
            }
        }
        let url =
            url::Url::parse(&format!("{}://{}:{}", ws_scheme, host, port))?.join(WS_ENDPOINT)?;
        Ok(url)
    }

    pub async fn run_forever(&mut self) -> Result<(), Box<dyn Error>> {
        let url = self.parse_url().await?;
        let _ = self
            .logger
            .log(LogType::Info, &format!("Connecting to master on [{}]", url))
            .await;

        let (tx, rx) = futures_channel::mpsc::unbounded();
        let (ws_stream, _) = match connect_async(url).await {
            Ok((ws_stream, res)) => (ws_stream, res),
            Err(e) => {
                return Err(Box::new(e));
            }
        };
        self.set_status(Status::Connected);
        let _ = self.logger.log(LogType::Info, "Connected to master").await;
        let (write, read) = ws_stream.split();
        self.tx = Arc::new(RwLock::new(Some(tx)));

        let ws_out = rx.map(Ok).forward(write);
        let ws_in = {
            read.for_each(|message| async {
                if let Ok(msg) = message {
                    match msg {
                        Message::Text(text) => {
                            if let Ok(ws_message) = ControlWebSocketMessage::from_json(&text) {
                                match ws_message {
                                    ControlWebSocketMessage::Create(mut test) => {
                                        test.set_logger(self.logger.clone());
                                        test.set_print_stats_to_console(
                                            self.print_stats_to_console,
                                        );
                                        test.set_run_time(None);
                                        self.logger.log_buffered(
                                            LogType::Info,
                                            &format!(
                                                "Creating Test with [{}] users",
                                                test.get_user_count()
                                            ),
                                        );
                                        *self.test.write() = Some(test);
                                    }

                                    ControlWebSocketMessage::Start => {
                                        self.logger.log_buffered(LogType::Info, "Starting test");
                                        self.run_test();
                                    }

                                    ControlWebSocketMessage::Stop => {
                                        self.logger.log_buffered(LogType::Info, "Stopping test");
                                        self.stop();
                                    }

                                    ControlWebSocketMessage::Finish => {
                                        self.logger.log_buffered(LogType::Info, "Finishing test");
                                        self.finish();
                                    }

                                    _ => {}
                                }
                            } else {
                                self.logger.log_buffered(
                                    LogType::Error,
                                    &format!("Invalid message: {}", text),
                                );
                            }
                        }
                        Message::Close(_) => {
                            self.logger
                                .log_buffered(LogType::Info, "Closing connection");
                            self.logger.log_buffered(LogType::Info, "Stopping test");
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
        self.set_status(Status::Running);
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
                .log_buffered(LogType::Info, "Waiting for background thread to terminate");
            match background_join_handle.await {
                Ok(_) => {}
                Err(e) => {
                    self.logger.log_buffered(
                        LogType::Error,
                        &format!("Error while joining background thread: {}", e),
                    );
                }
            }
        }

        if let Some(test_handle) = test_handle {
            self.logger
                .log_buffered(LogType::Info, "Waiting for test to terminate");
            match test_handle.await {
                Ok(_) => {}
                Err(e) => {
                    self.logger
                        .log_buffered(LogType::Error, &format!("Error while joining test: {}", e));
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
                let test = self.test.read().clone();
                if let Some(_) = test {
                    let results_websocket_message = self.create_results_websocket_message();
                    if let Some(results_websocket_message) = results_websocket_message{
                        let message = ControlWebSocketMessage::Update(results_websocket_message);
                        if let Some(json) = message.into_json() {
                            if tx.unbounded_send(Message::text(json)).is_err() {}
                        }
                    }

                }
            }
            tokio::time::sleep(Duration::from_secs(thread_sleep_time)).await;
        }
    }

    pub fn get_test(&self) -> Option<Test> {
        self.test.read().clone()
    }
}

#[async_trait]
impl Runnable for Worker {
    async fn run(&mut self) {
        let token = self.token.lock().unwrap().clone();
        select! {
            _ = token.cancelled() => {
            }
            res = self.run_forever() => {
                match res{
                    Ok(_) => {}
                    Err(e) => {
                        self.logger.log_buffered(LogType::Error, &format!("Error while running: {}", e));
                        self.set_status(Status::Error(e.to_string()));
                    }
                }
            }
        }
        self.join_handles().await;
        self.logger
            .log_buffered(LogType::Info, "Terminating... Bye!");
        //flush buffer
        let _ = self.logger.flush_buffer().await;
    }

    fn stop(&self) {
        self.set_status(Status::Stopped);
        self.stop_test();
        self.token.lock().unwrap().cancel();
    }

    fn finish(&self) {
        self.set_status(Status::Finished);
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
