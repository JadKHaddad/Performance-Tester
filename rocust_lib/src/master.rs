use crate::test::Test;
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fmt;
use std::sync::Arc;
use tokio::sync::broadcast;

use futures_util::{SinkExt, StreamExt};
use poem::{
    middleware::Tracing,
    get, handler,
    listener::TcpListener,
    web::{
        websocket::{Message, WebSocket},
        Data, Html, Path,
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
                    if sender.send(format!("{}",text)).is_err() {
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
            }

            while let Ok(msg) = receiver.recv().await {
                if sink.send(Message::Text(msg)).await.is_err() {
                    break;
                }
            }
        });
    })
}
#[handler]
fn index() -> Html<&'static str> {
    Html(
        r###"
    <body>
        <form id="loginForm">
            Name: <input id="nameInput" type="text" />
            <button type="submit">Login</button>
        </form>
        
        <form id="sendForm" hidden>
            Text: <input id="msgInput" type="text" />
            <button type="submit">Send</button>
        </form>
        
        <textarea id="msgsArea" cols="50" rows="30" hidden></textarea>
    </body>
    <script>
        let ws;
        const loginForm = document.querySelector("#loginForm");
        const sendForm = document.querySelector("#sendForm");
        const nameInput = document.querySelector("#nameInput");
        const msgInput = document.querySelector("#msgInput");
        const msgsArea = document.querySelector("#msgsArea");
        
        nameInput.focus();
        loginForm.addEventListener("submit", function(event) {
            event.preventDefault();
            loginForm.hidden = true;
            sendForm.hidden = false;
            msgsArea.hidden = false;
            msgInput.focus();
            ws = new WebSocket("ws://127.0.0.1:3000/ws");
            ws.onmessage = function(event) {
                msgsArea.value += event.data + "\r\n";
            }
        });
        
        sendForm.addEventListener("submit", function(event) {
            event.preventDefault();
            ws.send(msgInput.value);
            msgInput.value = "";
        });
    </script>
    "###,
    )
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
    pub async fn run_forever(&self) -> Result<(), std::io::Error>  {
        if std::env::var_os("RUST_LOG").is_none() {
            std::env::set_var("RUST_LOG", "poem=debug");
        }
        tracing_subscriber::fmt::init();

        let app = Route::new()
        .at("/", get(index))
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
