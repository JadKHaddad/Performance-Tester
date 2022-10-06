
use tokio::sync::broadcast;
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fmt;
use std::sync::Arc;
use crate::test::Test;


#[derive(Debug, Deserialize, Serialize)]
pub enum WebSocketMessage {
    Create(Test),
    Start,
    Stop,
}

impl WebSocketMessage {
    pub fn from_json(json: &str) -> Result<Self, Box<dyn Error>> {
        let message: Self =  serde_json::from_str(json)?;
        Ok(message)
    }

    pub fn to_json(&self) -> Result<String, Box<dyn Error>> {
        let json = serde_json::to_string(self)?;
        Ok(json)
    }
}

impl fmt::Display for WebSocketMessage{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WebSocketMessage::Create(test) => write!(f, "Create({})", test),
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
    host: [u8; 4],
    port: u16,
    state: Arc<AppState>,
}

impl Master {
    pub fn new(workers_count: u32, test: Test, host: [u8; 4], port: u16) -> Master {
        let (tx, _rx) = broadcast::channel(100);
        let state = Arc::new(AppState { test, tx });
        Master {
            workers_count,
            host,
            port,
            state,
        }
    }

    // the master will wait for the workers to connect. then he will send them the test to run and tell each one of them how many users to run.
    // the workers will run the test and send the results back to the master.
    // the master will aggregate the results.
    pub async fn run_forever(&self) {
        
    }

    pub fn run(&mut self) {
        todo!()
    }

    pub fn stop(&mut self) {
        todo!()
    }
}
