use parking_lot::RwLock;
use rand::Rng;
use reqwest::Client;
use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use std::time::Instant;
use tokio::select;
use tokio::time::error::Elapsed;
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;

#[derive(Clone, Debug)]
pub enum Method {
    GET,
    POST,
    PUT,
    DELETE,
}

impl fmt::Display for Method {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Method::GET => write!(f, "GET"),
            Method::POST => write!(f, "POST"),
            Method::PUT => write!(f, "PUT"),
            Method::DELETE => write!(f, "DELETE"),
        }
    }
}
#[derive(Clone, Debug)]
pub struct EndPointResults {
    total_requests: u32,
    total_response_time: u32,
    average_response: u32,
}

impl EndPointResults {
    pub fn new() -> EndPointResults {
        EndPointResults {
            total_requests: 0,
            total_response_time: 0,
            average_response: 0,
        }
    }

    pub fn add_response_time(&mut self, response_time: u32) {
        self.total_response_time += response_time;
        self.total_requests += 1;
        self.average_response = self.total_response_time / self.total_requests;
    }
}

impl fmt::Display for EndPointResults {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Total Response Time [{}] | Total Requests [{}] | Average Response [{}]",
            self.total_requests, self.total_response_time, self.average_response
        )
    }
}

#[derive(Clone, Debug)]
pub struct EndPoint {
    method: Method,
    url: String,
    results: Arc<RwLock<EndPointResults>>,
}

impl EndPoint {
    pub fn new(method: Method, url: String) -> EndPoint {
        EndPoint {
            method,
            url,
            results: Arc::new(RwLock::new(EndPointResults::new())),
        }
    }

    pub fn get_method(&self) -> &Method {
        &self.method
    }

    pub fn get_url(&self) -> &String {
        &self.url
    }

    fn add_response_time(&self, response_time: u32) {
        self.results.write().add_response_time(response_time);
    }
}

impl fmt::Display for EndPoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Method [{}] | Url [{}] | Results [{}]",
            self.method,
            self.url,
            self.results.read()
        )
    }
}

#[derive(Clone, Debug)]
pub struct TestData {
    //TODO
}

#[derive(Clone, Debug)]
pub struct Test {
    client: Client,
    token: Arc<Mutex<CancellationToken>>,
    users: u32,
    run_time: Option<u64>,
    sleep: u64,
    host: String,
    end_points: Vec<EndPoint>,
    global_headers: Option<HashMap<String, String>>,
    results: Arc<RwLock<EndPointResults>>,
    start_timestamp: Option<Instant>,
    end_timestamp: Option<Instant>,
}

impl Test {
    pub fn new(
        users: u32,
        run_time: Option<u64>,
        sleep: u64,
        host: String,
        end_points: Vec<EndPoint>,
        global_headers: Option<HashMap<String, String>>,
    ) -> Self {
        Self {
            client: Client::new(),
            token: Arc::new(Mutex::new(CancellationToken::new())),
            users,
            run_time,
            sleep,
            host,
            end_points,
            global_headers,
            results: Arc::new(RwLock::new(EndPointResults::new())),
            start_timestamp: None,
            end_timestamp: None,
        }
    }

    pub fn create_handler(&self) -> Self {
        self.clone()
    }

    fn add_response_time(&self, response_time: u32) {
        self.results.write().add_response_time(response_time);
    }

    pub async fn select_run_mode_and_run(&mut self) -> Result<(), Elapsed> {
        self.start_timestamp = Some(Instant::now());
        match self.run_time {
            Some(run_time) => {
                return self.run_with_timeout(run_time).await;
            }
            None => {
                self.run_forever().await;
                return Ok(());
            }
        }
    }

    pub async fn run(mut self) -> Self {
        let token = self.token.lock().unwrap().clone();
        select! {
            _ = token.cancelled() => {
                println!("canceled");
                self.end_timestamp = Some(Instant::now());
                self
            }
            _ = self.select_run_mode_and_run() => {
                self.end_timestamp = Some(Instant::now());
                self
            }
        }
    }

    pub async fn run_with_timeout(&self, time_out: u64) -> Result<(), Elapsed> {
        let future = self.run_forever();
        timeout(Duration::from_secs(time_out), future).await
    }

    pub async fn run_forever(&self) {
        let mut handles = vec![];
        for i in 0..self.users {
            let user_id = i + 1;
            println!("spwaning user: {}", user_id);
            let test = self.clone();
            let handle = tokio::spawn(async move {
                loop {
                    let end_point = test.select_random_end_point();
                    let url = format!("{}{}", test.host, end_point.get_url());

                    let mut request = match end_point.get_method() {
                        Method::GET => test.client.get(&url),
                        Method::POST => test.client.post(&url),
                        Method::PUT => test.client.put(&url),
                        Method::DELETE => test.client.delete(&url),
                    };

                    if let Some(global_headers) = &test.global_headers {
                        for (key, value) in global_headers {
                            request = request.header(key, value);
                        }
                    }

                    let start = Instant::now();
                    // TODO ConnectionErrors are not handled here yet
                    if let Ok(response) = request.send().await {
                        let duration = start.elapsed();
                        println!(
                            "user: {} | {} {} | {:?} | thread id: {:?}",
                            user_id,
                            response.status(),
                            url,
                            duration,
                            thread::current().id()
                        );
                        end_point.add_response_time(duration.as_millis() as u32);
                        test.add_response_time(duration.as_millis() as u32);
                    }
                    tokio::time::sleep(std::time::Duration::from_secs(test.select_random_sleep()))
                        .await;
                }
            });
            handles.push(handle);
        }
        println!("all users have been spawned");
        for handle in handles {
            match handle.await {
                Ok(_) => {}
                Err(e) => {
                    println!("error: {}", e);
                }
            }
        }
    }

    pub fn select_random_end_point(&self) -> &EndPoint {
        let mut rng = rand::thread_rng();
        let index = rng.gen_range(0..self.end_points.len());
        &self.end_points[index]
    }

    pub fn select_random_sleep(&self) -> u64 {
        let mut rng = rand::thread_rng();
        rng.gen_range(0..self.sleep)
    }

    pub fn stop(&self) {
        println!("canceling");
        self.token.lock().unwrap().cancel();
    }

    pub fn get_elapsed_time(&self) -> Option<Duration> {
        match (self.start_timestamp, self.end_timestamp) {
            (Some(start), Some(end)) => Some(end.duration_since(start)),
            _ => None,
        }
    }
}

impl fmt::Display for Test {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Users [{}] | RunTime [{}] | Sleep [{}] | Host [{}] | EndPoints [{:?}] | GlobalHeaders [{:?}] | Results [{}] | StartTimestamp [{:?}] | EndTimestamp [{:?}] | ElapsedTime [{:?}]",
            self.users,
            self.run_time.unwrap_or(0),
            self.sleep,
            self.host,
            self.end_points,
            self.global_headers.as_ref().unwrap_or(&HashMap::new()),
            self.results.read(),
            self.start_timestamp,
            self.end_timestamp,
            self.get_elapsed_time()
        )
    }
}

#[tokio::main]
async fn main() {
    let test = Test::new(
        9,
        None,
        5,
        "https://httpbin.org".to_string(),
        vec![
            EndPoint::new(Method::GET, "/get".to_string()),
            EndPoint::new(Method::POST, "/post".to_string()),
            EndPoint::new(Method::PUT, "/put".to_string()),
            EndPoint::new(Method::DELETE, "/delete".to_string()),
        ],
        None,
    );

    let test_handler = test.create_handler();
    tokio::spawn(async move {
        println!("canceling test in 5 seconds");
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        println!("attemting cancel");
        test_handler.stop();
    });

    let test = test.run().await;
    println!("{}", test);
}
