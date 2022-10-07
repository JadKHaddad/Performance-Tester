use crate::master::WebSocketMessage;
use crate::{Logger, LogType};
use crate::test::Test;
use futures_util::{future, pin_mut, StreamExt};
use parking_lot::RwLock;
use std::error::Error;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::select;
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};
use tokio_util::sync::CancellationToken;
pub struct Worker {
    test: Arc<RwLock<Option<Test>>>,
    master_addr: String,
    token: Arc<Mutex<CancellationToken>>,
    logger: Arc<Logger>,
}
//TODO: Background thread for the logger. Status for the worker.
impl Worker {
    pub fn new(master_addr: String, logfile_path: String) -> Worker {
        Worker {
            test: Arc::new(RwLock::new(None)),
            master_addr,
            token: Arc::new(Mutex::new(CancellationToken::new())),
            logger: Arc::new(Logger::new(logfile_path)),
        }
    }

    pub async fn run_forever(&self) -> Result<(), Box<dyn Error>> {
        //helper
        async fn read_stdin(tx: futures_channel::mpsc::UnboundedSender<Message>) {
            let mut stdin = tokio::io::stdin();
            loop {
                let mut buf = vec![0; 1024];
                let n = match stdin.read(&mut buf).await {
                    Err(_) | Ok(0) => break,
                    Ok(n) => n,
                };
                buf.truncate(n);
                if tx.unbounded_send(Message::binary(buf)).is_err() {
                    break;
                }
            }
        }

        let url = url::Url::parse(&self.master_addr)?;

        let (stdin_tx, stdin_rx) = futures_channel::mpsc::unbounded();
        tokio::spawn(read_stdin(stdin_tx));

        let (ws_stream, _) = connect_async(url).await?;
        let (write, read) = ws_stream.split();

        let stdin_to_ws = stdin_rx.map(Ok).forward(write);

        let ws_to_stdout = {
            read.for_each(|message| async {
                if let Ok(msg) = message {
                    match msg {
                        Message::Text(text) => {
                            if let Ok(ws_message) = WebSocketMessage::from_json(&text) {
                                match ws_message {
                                    WebSocketMessage::Create(mut test, user_count) => {
                                        test.set_logger(self.logger.clone());
                                        test.set_run_time(None);
                                        self.logger.log_buffered(LogType::INFO, &format!("Creating Test with [{}] users", user_count));
                                        *self.test.write() = Some(test);
                                    }

                                    WebSocketMessage::Start => {
                                        self.logger.log_buffered(LogType::INFO, "Starting test");
                                        self.run_test();
                                    }

                                    WebSocketMessage::Stop => {
                                        self.logger.log_buffered(LogType::INFO, "Stopping test");
                                        self.stop();
                                        self.logger.log_buffered(LogType::INFO, "Exiting");
                                    }

                                    WebSocketMessage::Finish => {
                                        self.logger.log_buffered(LogType::INFO, "Finishing test");
                                        self.finish();
                                        self.logger.log_buffered(LogType::INFO, "Exiting");
                                    }
                                }
                            } else {
                                self.logger.log_buffered(LogType::ERROR, "Invalid message");
                            }
                        }
                        Message::Close(_) => {
                            self.logger.log_buffered(LogType::INFO, "Closing connection");
                            self.logger.log_buffered(LogType::INFO, "Stopping test");
                            self.stop();
                            self.logger.log_buffered(LogType::INFO, "Exiting");
                        }
                        _ => {}
                    }
                }
            })
        };

        pin_mut!(stdin_to_ws, ws_to_stdout);
        future::select(stdin_to_ws, ws_to_stdout).await; // could totally use tokio::select!
        Ok(())
    }

    pub fn run_test(&self) {
        let test = self.test.read().clone();
        tokio::spawn(async move {
            if let Some(mut test) = test {
                test.run().await;
            }
        });
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
        self.stop_test();
        self.token.lock().unwrap().cancel();
    }

    pub fn finish(&self) {
        self.finish_test();
        self.token.lock().unwrap().cancel();
    }
}
