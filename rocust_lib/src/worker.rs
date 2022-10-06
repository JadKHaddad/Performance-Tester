use crate::master::WebSocketMessage;
use crate::test::Test;
use parking_lot::RwLock;
use std::error::Error;
use std::sync::Arc;
use websocket::ClientBuilder;
use websocket::OwnedMessage;

pub struct Worker {
    test: Arc<RwLock<Option<Test>>>,
    master_addr: String,
}

impl Worker {
    pub fn new(master_addr: String) -> Worker {
        Worker {
            test: Arc::new(RwLock::new(None)),
            master_addr,
        }
    }

    pub fn connect(&self) -> Result<(), Box<dyn Error>> {
        let url = format!("ws://{}/ws", self.master_addr);
        let client = ClientBuilder::new(&url)?.connect_insecure()?;

        let (mut receiver, mut sender) = client.split()?;

        for message in receiver.incoming_messages() {
            match message {
                Ok(message) => {
                    match message {
                        OwnedMessage::Text(text) => {
                            let ws_message = WebSocketMessage::from_json(&text)?;
                            match ws_message {
                                WebSocketMessage::Create(test, user_count) => {
                                    *self.test.write() = Some(test);
                                    println!("Test created");
                                }

                                WebSocketMessage::Start => {
                                    println!("Test started");
                                    self.run_test();
                                }

                                WebSocketMessage::Stop => {
                                    self.stop_test();
                                    println!("Test stopped");
                                }
                            }
                        }
                        OwnedMessage::Binary(data) => {
                            println!("Received: {:?}", data);
                        }
                        OwnedMessage::Ping(data) => {
                            println!("Received: {:?}", data);
                        }
                        OwnedMessage::Pong(data) => {
                            println!("Received: {:?}", data);
                        }
                        OwnedMessage::Close(_) => {
                            println!("Received: close");
                            println!("Exiting");
                            break;
                        }
                    }
                    //println!("{:?}", message)
                }
                Err(e) => {
                    println!("Error: {:?}", e);
                    break;
                }
            }
        }
        println!("Bye");
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

    pub fn run(&mut self) {
        todo!()
    }

    pub fn stop(&mut self) {
        todo!()
    }
}
