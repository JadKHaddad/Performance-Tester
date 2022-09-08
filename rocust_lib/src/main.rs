use parking_lot::RwLock;
use rand::Rng;
use reqwest::Client;
use reqwest::RequestBuilder;
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
pub enum Status {
    CREATED,
    RUNNING,
    STOPPED,
    FINISHED,
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Status::CREATED => write!(f, "CREATED"),
            Status::RUNNING => write!(f, "RUNNING"),
            Status::STOPPED => write!(f, "STOPPED"),
            Status::FINISHED => write!(f, "FINISHED"),
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
            "Total Requests [{}] | Total Response Time [{}] | Average Response [{}]",
            self.total_requests, self.total_response_time, self.average_response
        )
    }
}

#[derive(Clone, Debug)]
pub struct EndPoint {
    method: Method,
    url: String,
    results: Arc<RwLock<EndPointResults>>,
    headers: Option<HashMap<String, String>>,
}

impl EndPoint {
    pub fn new(method: Method, url: String, headers: Option<HashMap<String, String>>) -> EndPoint {
        EndPoint {
            method,
            url,
            results: Arc::new(RwLock::new(EndPointResults::new())),
            headers,
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
pub struct User {
    client: Client,
    id: String,
    sleep: u64,
    host: Arc<String>,
    global_endpoints: Arc<Vec<EndPoint>>,
    global_headers: Option<HashMap<String, String>>,
    global_results: Arc<RwLock<EndPointResults>>,
    results: Arc<RwLock<EndPointResults>>,
    endpoints: Arc<Vec<EndPoint>>, //TODO  make this a hashmap with the endpoint name as the key and the endpoint result as the value
}

impl fmt::Display for User {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "User [{}] | Results [{}] | Endpoints [{:?}]",
            self.id, self.results.read(), self.endpoints
        )
    }
}

impl User {
    pub fn new(
        id: String,
        sleep: u64,
        host: Arc<String>,
        global_endpoints: Arc<Vec<EndPoint>>,
        global_headers: Option<HashMap<String, String>>,
        global_results: Arc<RwLock<EndPointResults>>,
    ) -> User {
        User {
            client: Client::new(),
            id,
            sleep,
            host,
            global_endpoints,
            global_headers,
            global_results,
            results: Arc::new(RwLock::new(EndPointResults::new())),
            endpoints: Arc::new(Vec::new()),
        }
    }

    fn add_headers(&self, mut request: RequestBuilder, endpoint: &EndPoint) -> RequestBuilder {
        if let Some(global_headers) = &self.global_headers {
            for (key, value) in global_headers {
                request = request.header(key, value);
            }
        }
        if let Some(headers) = &endpoint.headers {
            for (key, value) in headers {
                request = request.header(key, value);
            }
        }
        request
    }

    async fn perform(&self) {
        loop {
            let endpoint = Test::select_random_endpoint(&self.global_endpoints);
            let url = format!("{}{}", self.host, endpoint.get_url());
            let mut request = match endpoint.get_method() {
                Method::GET => self.client.get(&url),
                Method::POST => self.client.post(&url),
                Method::PUT => self.client.put(&url),
                Method::DELETE => self.client.delete(&url),
            };
            request = self.add_headers(request, endpoint);
            let start = Instant::now();
            //TODO ConnectionErrors are not handled here yet
            if let Ok(response) = request.send().await {
                let duration = start.elapsed();
                println!(
                    "user: {} | {} {} | {:?} | thread id: {:?}",
                    self.id,
                    response.status(),
                    url,
                    duration,
                    thread::current().id()
                );
                endpoint.add_response_time(duration.as_millis() as u32);
                self.
                    add_response_time(duration.as_millis() as u32);
            }
            tokio::time::sleep(Duration::from_secs(Test::select_random_sleep(self.sleep))).await;
        }
    }
}

//this struct is used to control the test from another thread
#[derive(Clone, Debug)]
pub struct TestHandler {
    token: Arc<Mutex<CancellationToken>>,
    results: Arc<RwLock<EndPointResults>>,
}

impl TestHandler {
    pub fn new(
        token: Arc<Mutex<CancellationToken>>,
        results: Arc<RwLock<EndPointResults>>,
    ) -> TestHandler {
        TestHandler { token, results }
    }

    pub fn stop(&self) {
        self.token.lock().unwrap().cancel();
    }

    //change the test data
    //TODO
}

trait Updatble {
    fn add_response_time(&self, response_time: u32);
}

impl Updatble for Test {
    fn add_response_time(&self, response_time: u32) {
        self.results.write().add_response_time(response_time);
    }
}

impl Updatble for User {
    fn add_response_time(&self, response_time: u32) {
        self.global_results.write().add_response_time(response_time);
        self.results.write().add_response_time(response_time);
    }
}

impl Updatble for TestHandler {
    fn add_response_time(&self, response_time: u32) {
        self.results.write().add_response_time(response_time);
    }
}

#[derive(Clone, Debug)]
pub struct Test {
    status: Status,
    token: Arc<Mutex<CancellationToken>>,
    users: u32,
    run_time: Option<u64>,
    sleep: u64,
    host: Arc<String>,
    endpoints: Arc<Vec<EndPoint>>,
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
        endpoints: Vec<EndPoint>,
        global_headers: Option<HashMap<String, String>>,
    ) -> Self {
        Self {
            status: Status::CREATED,
            token: Arc::new(Mutex::new(CancellationToken::new())),
            users,
            run_time,
            sleep,
            host: Arc::new(host),
            endpoints: Arc::new(endpoints),
            global_headers,
            results: Arc::new(RwLock::new(EndPointResults::new())),
            start_timestamp: None,
            end_timestamp: None,
        }
    }

    pub fn creat_user(&self, id: String) -> User {
        User::new(
            id,
            self.sleep,
            self.host.clone(),
            self.endpoints.clone(),
            self.global_headers.clone(),
            self.results.clone(),
        )
    }

    pub fn create_handler(&self) -> TestHandler {
        TestHandler::new(self.token.clone(), self.results.clone())
    }

    pub async fn select_run_mode_and_run(&mut self) -> Result<(), Elapsed> {
        self.start_timestamp = Some(Instant::now());
        return match self.run_time {
            Some(run_time) => self.run_with_timeout(run_time).await,
            None => {
                self.run_forever().await;
                Ok(())
            }
        };
    }

    pub async fn run(mut self) -> Self {
        let token = self.token.lock().unwrap().clone();
        select! {
            _ = token.cancelled() => {
                println!("canceled");
                self.end_timestamp = Some(Instant::now());
                self.status = Status::STOPPED;
                self
            }
            _ = self.select_run_mode_and_run() => {
                self.end_timestamp = Some(Instant::now());
                self.status = Status::FINISHED;
                self
            }
        }
    }

    pub async fn run_with_timeout(&mut self, time_out: u64) -> Result<(), Elapsed> {
        let future = self.run_forever();
        timeout(Duration::from_secs(time_out), future).await
    }

    pub async fn run_forever(&mut self) {
        self.status = Status::RUNNING;
        let mut join_handles = vec![];
        for i in 0..self.users {
            let user_id = i + 1;
            println!("spawning user: {}", user_id);
            let user = self.creat_user(user_id.to_string());

            let join_handle = tokio::spawn(async move {
                user.perform().await;
            });
            join_handles.push(join_handle);
        }
        println!("all users have been spawned");
        for join_handle in join_handles {
            match join_handle.await {
                Ok(_) => {}
                Err(e) => {
                    println!("error: {}", e);
                }
            }
        }
    }

    pub fn select_random_endpoint(endpoints: &Arc<Vec<EndPoint>>) -> &EndPoint {
        let mut rng = rand::thread_rng();
        let index = rng.gen_range(0..endpoints.len());
        &endpoints[index]
    }

    pub fn select_random_sleep(sleep: u64) -> u64 {
        let mut rng = rand::thread_rng();
        rng.gen_range(0..sleep)
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
            "Status [{}] | Users [{}] | RunTime [{}] | Sleep [{}] | Host [{}] | EndPoints [{:?}] | GlobalHeaders [{:?}] | Results [{}] | StartTimestamp [{:?}] | EndTimestamp [{:?}] | ElapsedTime [{:?}]",
            self.status,
            self.users,
            self.run_time.unwrap_or(0),
            self.sleep,
            self.host,
            self.endpoints,
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
        Some(10),
        5,
        "https://httpbin.org".to_string(),
        vec![
            EndPoint::new(Method::GET, "/get".to_string(), None),
            EndPoint::new(Method::POST, "/post".to_string(), None),
            EndPoint::new(Method::PUT, "/put".to_string(), None),
            EndPoint::new(Method::DELETE, "/delete".to_string(), None),
        ],
        None,
    );

    let test_handler = test.create_handler();
    tokio::spawn(async move {
        println!("canceling test in 5 seconds");
        tokio::time::sleep(Duration::from_secs(5)).await;
        println!("attempting cancel");
        test_handler.stop();
    });

    let test = test.run().await;
    println!("{}", test);
}
