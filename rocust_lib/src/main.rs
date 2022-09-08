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
pub enum UserBehaviour {
    //TODO
    AGGRESSIVE,
    PASSIVE,
    LAZY,
}

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
    requests_per_second: f64,
}

impl EndPointResults {
    pub fn new() -> EndPointResults {
        EndPointResults {
            total_requests: 0,
            total_response_time: 0,
            average_response: 0,
            requests_per_second: 0.0,
        }
    }

    pub fn add_response_time(&mut self, response_time: u32) {
        self.total_response_time += response_time;
        self.total_requests += 1;
        self.average_response = self.total_response_time / self.total_requests;
    }

    fn get_total_requests(&self) -> u32 {
        self.total_requests
    }

    fn set_requests_per_second(&mut self, requests_per_second: f64) {
        self.requests_per_second = requests_per_second;
    }

    fn calculate_requests_per_second(&mut self, elapsed: &Duration) {
        let total_requests = self.get_total_requests();
        let requests_per_second = total_requests as f64 / elapsed.as_secs_f64();
        self.set_requests_per_second(requests_per_second);
    }
}

impl fmt::Display for EndPointResults {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Total Requests [{}] | Requests per Second [{}] | Total Response Time [{}] | Average Response Time [{}]",
            self.total_requests, self.requests_per_second, self.total_response_time, self.average_response
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
    token: Arc<Mutex<CancellationToken>>,
    status: Arc<RwLock<Status>>,
    id: String,
    sleep: u64,
    host: Arc<String>,
    global_endpoints: Arc<Vec<EndPoint>>,
    global_headers: Option<HashMap<String, String>>,
    global_results: Arc<RwLock<EndPointResults>>,
    results: Arc<RwLock<EndPointResults>>,
    endpoints: Arc<RwLock<HashMap<String, EndPointResults>>>,
}

impl fmt::Display for User {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "User [{}] | Status [{}] | Results [{}]",
            self.id,
            self.status.read(),
            self.results.read()
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
            token: Arc::new(Mutex::new(CancellationToken::new())),
            status: Arc::new(RwLock::new(Status::CREATED)),
            id,
            sleep,
            host,
            global_endpoints,
            global_headers,
            global_results,
            results: Arc::new(RwLock::new(EndPointResults::new())),
            endpoints: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn get_endpoints(&self) -> Arc<RwLock<HashMap<String, EndPointResults>>> {
        self.endpoints.clone()
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

    fn add_endpoint_response_time(&self, response_time: u32, endpoint: &EndPoint) {
        endpoint.add_response_time(response_time);
        self.endpoints
            .write()
            .entry(endpoint.url.clone())
            .or_insert(EndPointResults::new())
            .add_response_time(response_time);
        self.add_response_time(response_time);
    }

    pub async fn run(&mut self) {
        let token = self.token.lock().unwrap().clone();
        select! {
            _ = token.cancelled() => {
                self.set_status(Status::STOPPED);
            }
            _ = self.run_forever() => {
                self.set_status(Status::FINISHED);
            }
        }
    }

    fn set_status(&self, status: Status) {
        *self.status.write() = status;
    }

    fn set_status_with_check(&self, status: Status) {
        let current_status = self.status.read().clone();
        match current_status {
            Status::RUNNING => {
                *self.status.write() = status;
            }
            _ => {}
        }
    }

    async fn run_forever(&mut self) {
        self.set_status(Status::RUNNING);
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
                self.add_endpoint_response_time(duration.as_millis() as u32, endpoint);
            }
            tokio::time::sleep(Duration::from_secs(Test::select_random_sleep(self.sleep))).await;
        }
    }

    pub fn stop(&self) {
        self.token.lock().unwrap().cancel();
    }

    pub fn create_handler(&self) -> UserHandler {
        UserHandler::new(self.token.clone(), self.results.clone())
    }
}

//this struct is used to control a user from another thread
#[derive(Clone, Debug)]
pub struct UserHandler {
    token: Arc<Mutex<CancellationToken>>,
    results: Arc<RwLock<EndPointResults>>,
}

impl UserHandler {
    pub fn new(
        token: Arc<Mutex<CancellationToken>>,
        results: Arc<RwLock<EndPointResults>>,
    ) -> UserHandler {
        UserHandler { token, results }
    }

    pub fn stop(&self) {
        self.token.lock().unwrap().cancel();
    }

    //change the user data
    //TODO
}

//this struct is used to control a test from another thread
#[derive(Clone, Debug)]
pub struct TestHandler {
    token: Arc<Mutex<CancellationToken>>,
    results: Arc<RwLock<EndPointResults>>,
    users: Arc<RwLock<Vec<User>>>,
    pub status: Arc<RwLock<Status>>,
    start_timestamp: Arc<RwLock<Option<Instant>>>,
    end_timestamp: Arc<RwLock<Option<Instant>>>,
    endpoints: Arc<Vec<EndPoint>>,
}

impl TestHandler {
    pub fn new(
        token: Arc<Mutex<CancellationToken>>,
        results: Arc<RwLock<EndPointResults>>,
        users: Arc<RwLock<Vec<User>>>,
        status: Arc<RwLock<Status>>,
        start_timestamp: Arc<RwLock<Option<Instant>>>,
        end_timestamp: Arc<RwLock<Option<Instant>>>,
        endpoints: Arc<Vec<EndPoint>>,
    ) -> TestHandler {
        TestHandler {
            token,
            results,
            users,
            status,
            start_timestamp,
            end_timestamp,
            endpoints,
        }
    }

    pub fn stop(&self) {
        self.token.lock().unwrap().cancel();
    }

    pub fn stop_a_user(&mut self, user_id: usize) {
        if let Some(user) = self.users.read().get(user_id) {
            user.stop();
        }
    }

    //change the test data
    //TODO
}

trait Updatble {
    fn add_response_time(&self, response_time: u32);
    fn set_requests_per_second(&self, requests_per_second: f64);
    fn calculate_requests_per_second(&self, elapsed: &Duration);
    fn get_results(&self) -> Arc<RwLock<EndPointResults>>;
}

impl Updatble for Test {
    fn add_response_time(&self, response_time: u32) {
        self.results.write().add_response_time(response_time);
    }

    fn set_requests_per_second(&self, requests_per_second: f64) {
        self.results
            .write()
            .set_requests_per_second(requests_per_second);
    }

    fn calculate_requests_per_second(&self, elapsed: &Duration) {
        self.results.write().calculate_requests_per_second(elapsed);
        for user in self.users.read().iter() {
            user.calculate_requests_per_second(elapsed);
        }
        for endpoint in self.endpoints.iter() {
            endpoint.calculate_requests_per_second(elapsed);
        }
    }

    fn get_results(&self) -> Arc<RwLock<EndPointResults>> {
        self.results.clone()
    }
}

impl Updatble for EndPoint {
    fn add_response_time(&self, response_time: u32) {
        self.results.write().add_response_time(response_time);
    }

    fn set_requests_per_second(&self, requests_per_second: f64) {
        self.results
            .write()
            .set_requests_per_second(requests_per_second);
    }

    fn calculate_requests_per_second(&self, elapsed: &Duration) {
        self.results.write().calculate_requests_per_second(elapsed);
    }

    fn get_results(&self) -> Arc<RwLock<EndPointResults>> {
        self.results.clone()
    }
}

impl Updatble for User {
    fn add_response_time(&self, response_time: u32) {
        self.global_results.write().add_response_time(response_time);
        self.results.write().add_response_time(response_time);
    }

    fn set_requests_per_second(&self, requests_per_second: f64) {
        self.results
            .write()
            .set_requests_per_second(requests_per_second);
    }

    fn calculate_requests_per_second(&self, elapsed: &Duration) {
        self.results.write().calculate_requests_per_second(elapsed);
        for (_, endpoint_result) in self.endpoints.write().iter_mut() {
            endpoint_result.calculate_requests_per_second(elapsed);
        }
    }

    fn get_results(&self) -> Arc<RwLock<EndPointResults>> {
        self.results.clone()
    }
}

impl Updatble for TestHandler {
    fn add_response_time(&self, response_time: u32) {
        self.results.write().add_response_time(response_time);
    }

    fn set_requests_per_second(&self, requests_per_second: f64) {
        self.results
            .write()
            .set_requests_per_second(requests_per_second);
    }

    fn calculate_requests_per_second(&self, elapsed: &Duration) {
        self.results.write().calculate_requests_per_second(elapsed);
        for user in self.users.read().iter() {
            user.calculate_requests_per_second(elapsed);
        }
        for endpoint in self.endpoints.iter() {
            endpoint.calculate_requests_per_second(elapsed);
        }
    }

    fn get_results(&self) -> Arc<RwLock<EndPointResults>> {
        self.results.clone()
    }
}

impl Updatble for UserHandler {
    fn add_response_time(&self, response_time: u32) {
        self.results.write().add_response_time(response_time);
    }

    fn set_requests_per_second(&self, requests_per_second: f64) {
        self.results
            .write()
            .set_requests_per_second(requests_per_second);
    }

    fn calculate_requests_per_second(&self, _elapsed: &Duration) {}

    fn get_results(&self) -> Arc<RwLock<EndPointResults>> {
        self.results.clone()
    }
}

#[derive(Clone, Debug)]
pub struct Test {
    status: Arc<RwLock<Status>>,
    token: Arc<Mutex<CancellationToken>>,
    user_count: u32,
    run_time: Option<u64>,
    sleep: u64,
    host: Arc<String>,
    endpoints: Arc<Vec<EndPoint>>,
    global_headers: Option<HashMap<String, String>>,
    results: Arc<RwLock<EndPointResults>>,
    start_timestamp: Arc<RwLock<Option<Instant>>>,
    end_timestamp: Arc<RwLock<Option<Instant>>>,
    users: Arc<RwLock<Vec<User>>>,
}

impl Test {
    pub fn new(
        user_count: u32,
        run_time: Option<u64>,
        sleep: u64,
        host: String,
        endpoints: Vec<EndPoint>,
        global_headers: Option<HashMap<String, String>>,
    ) -> Self {
        Self {
            status: Arc::new(RwLock::new(Status::CREATED)),
            token: Arc::new(Mutex::new(CancellationToken::new())),
            user_count,
            run_time,
            sleep,
            host: Arc::new(host),
            endpoints: Arc::new(endpoints),
            global_headers,
            results: Arc::new(RwLock::new(EndPointResults::new())),
            start_timestamp: Arc::new(RwLock::new(None)),
            end_timestamp: Arc::new(RwLock::new(None)),
            users: Arc::new(RwLock::new(Vec::new())),
        }
    }

    pub fn create_user(&self, id: String) -> User {
        let user = User::new(
            id,
            self.sleep,
            self.host.clone(),
            self.endpoints.clone(),
            self.global_headers.clone(),
            self.results.clone(),
        );
        self.users.write().push(user.clone());
        user
    }

    pub fn get_users(&self) -> Arc<RwLock<Vec<User>>> {
        self.users.clone()
    }

    pub fn get_endpoints(&self) -> Arc<Vec<EndPoint>> {
        self.endpoints.clone()
    }

    pub fn create_handler(&self) -> TestHandler {
        TestHandler::new(
            self.token.clone(),
            self.results.clone(),
            self.users.clone(),
            self.status.clone(),
            self.start_timestamp.clone(),
            self.end_timestamp.clone(),
            self.endpoints.clone(),
        )
    }

    pub async fn select_run_mode_and_run(&mut self) -> Result<(), Elapsed> {
        *self.start_timestamp.write() = Some(Instant::now()); //TODO use a method just like the line after this one
        self.set_status(Status::RUNNING);
        self.update_in_background(2);
        return match self.run_time {
            Some(run_time) => self.run_with_timeout(run_time).await,
            None => {
                self.run_forever().await;
                Ok(())
            }
        };
    }

    pub async fn run(&mut self) {
        let token = self.token.lock().unwrap().clone();
        select! {
            _ = token.cancelled() => {
                *self.end_timestamp.write() = Some(Instant::now());
                self.set_status(Status::STOPPED);
                self.set_users_status(Status::STOPPED);
            }
            _ = self.select_run_mode_and_run() => {
                *self.end_timestamp.write() = Some(Instant::now());
                self.set_status(Status::FINISHED);
                self.set_users_status(Status::FINISHED);
            }
        }
    }

    fn set_status(&self, status: Status) {
        *self.status.write() = status;
    }

    fn set_users_status(&self, status: Status) {
        for user in self.users.write().iter_mut() {
            user.set_status_with_check(status.clone());
        }
    }

    fn update_in_background(&self, thread_sleep_time: u64) {
        let test_handler = self.create_handler();
        tokio::spawn(async move {
            loop {
                if let Some(elapsed) = Test::calculate_elapsed_time(
                    *test_handler.start_timestamp.read(),
                    *test_handler.end_timestamp.read(),
                ) {
                    test_handler.calculate_requests_per_second(&elapsed);
                }
                tokio::time::sleep(Duration::from_secs(thread_sleep_time)).await;
            }
        });
    }

    pub async fn run_with_timeout(&mut self, time_out: u64) -> Result<(), Elapsed> {
        let future = self.run_forever();
        timeout(Duration::from_secs(time_out), future).await
    }

    pub async fn run_forever(&mut self) {
        let mut join_handles = vec![];
        for i in 0..self.user_count {
            let user_id = i;
            println!("spawning user: {}", user_id);
            let mut user = self.create_user(user_id.to_string());
            let join_handle = tokio::spawn(async move {
                user.run().await;
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
        self.token.lock().unwrap().cancel();
    }

    pub fn stop_a_user(&mut self, user_id: usize) {
        if let Some(user) = self.users.read().get(user_id) {
            user.stop();
        }
    }

    pub fn get_elapsed_time(&self) -> Option<Duration> {
        match (*self.start_timestamp.read(), *self.end_timestamp.read()) {
            (Some(start), Some(end)) => Some(end.duration_since(start)),
            (Some(start), None) => Some(Instant::now().duration_since(start)),
            _ => None,
        }
    }

    fn calculate_elapsed_time(
        start_timestamp: Option<Instant>,
        end_timestamp: Option<Instant>,
    ) -> Option<Duration> {
        match (start_timestamp, end_timestamp) {
            (Some(start), Some(end)) => Some(end.duration_since(start)),
            (Some(start), None) => Some(Instant::now().duration_since(start)),
            _ => None,
        }
    }
}

impl fmt::Display for Test {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Status [{}] | Users [{}] | RunTime [{}] | Sleep [{}] | Host [{}] | GlobalHeaders [{:?}] | Results [{}] | StartTimestamp [{:?}] | EndTimestamp [{:?}] | ElapsedTime [{:?}]",
            self.status.read(),
            self.user_count,
            self.run_time.unwrap_or(0),
            self.sleep,
            self.host,
            self.global_headers.as_ref().unwrap_or(&HashMap::new()),
            self.results.read(),
            self.start_timestamp,
            self.end_timestamp,
            self.get_elapsed_time()
        )
    }
}

#[tokio::main(flavor = "multi_thread", worker_threads = 1000)]
async fn main() {
    let mut test = Test::new(
        10,
        Some(20),
        5,
        "https://google.com".to_string(),
        vec![
            EndPoint::new(Method::GET, "/".to_string(), None),
            EndPoint::new(Method::GET, "/get".to_string(), None),
            EndPoint::new(Method::POST, "/post".to_string(), None),
            EndPoint::new(Method::PUT, "/put".to_string(), None),
            EndPoint::new(Method::DELETE, "/delete".to_string(), None),
        ],
        None,
    );
    println!("test created {}", test);

    let test_handler = test.create_handler();
    tokio::spawn(async move {
        println!("canceling test in 200 seconds");
        tokio::time::sleep(Duration::from_secs(30)).await;
        println!("attempting cancel");
        test_handler.stop();
    });

    let mut test_handler = test.create_handler();
    tokio::spawn(async move {
        println!("canceling user 1 in 5 seconds");
        tokio::time::sleep(Duration::from_secs(5)).await;
        println!("attempting cancel user 1");
        test_handler.stop_a_user(1);
    });

    let mut test_handler = test.create_handler();
    tokio::spawn(async move {
        println!("canceling user 15 in 7 seconds");
        tokio::time::sleep(Duration::from_secs(7)).await;
        println!("attempting cancel user 15");
        test_handler.stop_a_user(15);
    });

    let test_handler = test.create_handler();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(3)).await;
            println!("STATUS: [{}]", test_handler.status.read());
        }
    });

    test.run().await;
    println!("{}", test);

    let endpoints = test.get_endpoints();
    for endpoint in endpoints.iter() {
        println!("{}", endpoint);
        println!("------------------------------");
    }
    let users = test.get_users();
    for user in users.read().iter() {
        println!("{}\n", user);
        for (endpoint_url, results) in user.get_endpoints().read().iter() {
            println!("\t[{}] | [{}]\n", endpoint_url, results);
        }
        println!("------------------------------");
    }
}
