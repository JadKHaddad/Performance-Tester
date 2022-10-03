use std::net::TcpStream;

use crate::test::Test;
use std::error::Error;
use websocket::ClientBuilder;
use websocket::{Message, OwnedMessage};

pub struct Worker {
    test: Option<Test>,
    host: [u8; 4],
    port: u16,
    //client: Option<websocket::sync::Client<std::net::TcpStream>>,
}

impl Worker {
    pub fn new(test: Option<Test>, host: [u8; 4], port: u16) -> Worker {
        Worker { test, host, port }
    }

    fn create_host_string_from_host_ip(&self) -> String {
        format!(
            "{}.{}.{}.{}",
            self.host[0], self.host[1], self.host[2], self.host[3]
        )
    }

    pub fn connect(&self) -> Result<(), Box<dyn Error>> {
        let url = format!(
            "ws://{}:{}/ws",
            self.create_host_string_from_host_ip(),
            self.port
        );
        let client = ClientBuilder::new(&url)?.connect_insecure()?;

        let (mut receiver, mut sender) = client.split()?;

        for message in receiver.incoming_messages() {
            match message {
                Ok(message) => {
                    match message {
                        OwnedMessage::Text(text) => {
                            println!("Received: {}", text);
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

        Ok(())
    }

    pub async fn run_forever(&mut self) {
        if let Some(ref mut test) = self.test {
            test.run().await;
        }
    }

    pub fn run(&mut self) {
        todo!()
    }

    pub fn stop(&mut self) {
        todo!()
    }
}
