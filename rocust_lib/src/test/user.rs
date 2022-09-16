use crate::{EndPoint, LogType, Logger, Method, Results, Status, Updatble};
use parking_lot::RwLock;
use rand::Rng;
use reqwest::Client;
use reqwest::RequestBuilder;
use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, Mutex};
//use std::thread;
use std::time::Duration;
use std::time::Instant;
use tokio::select;
use tokio_util::sync::CancellationToken;

#[derive(Clone, Debug)]
pub enum UserBehaviour {
    //TODO
    AGGRESSIVE,
    PASSIVE,
    LAZY,
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
    global_results: Arc<RwLock<Results>>,
    results: Arc<RwLock<Results>>,
    endpoints: Arc<RwLock<HashMap<String, Results>>>,
    logger: Arc<Logger>,
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
        global_results: Arc<RwLock<Results>>,
        logger: Arc<Logger>,
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
            results: Arc::new(RwLock::new(Results::new())),
            endpoints: Arc::new(RwLock::new(HashMap::new())),
            logger,
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

    pub async fn run(&mut self) {
        let token = self.token.lock().unwrap().clone();
        select! {
            _ = token.cancelled() => {

            }
            _ = self.run_forever() => {
            }
        }
        self.logger
            .log_buffed(LogType::INFO, &format!("User [{}] stopped", self.id));
    }

    async fn run_forever(&mut self) {
        self.set_status(Status::RUNNING);
        loop {
            let endpoint = self.select_random_endpoint();
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
                self.logger.log_buffed(
                    LogType::INFO,
                    &format!(
                        "User: [{}] | {} {} | {:?}",
                        self.id,
                        response.status(),
                        url,
                        duration
                    ),
                );

                self.add_endpoint_response_time(duration.as_millis() as u32, endpoint);
            }
            tokio::time::sleep(Duration::from_secs(self.select_random_sleep())).await;
        }
    }

    pub fn stop(&self) {
        self.token.lock().unwrap().cancel();
        self.set_status(Status::STOPPED);
    }

    pub fn finish(&self) {
        self.token.lock().unwrap().cancel();
        let current_status = self.status.read().clone();
        match current_status {
            Status::STOPPED => {}
            _ => {
                self.set_status(Status::FINISHED);
            }
        }
    }

    fn set_status(&self, status: Status) {
        *self.status.write() = status;
    }

    fn select_random_endpoint(&self) -> &EndPoint {
        let mut rng = rand::thread_rng();
        let index = rng.gen_range(0..self.global_endpoints.len());
        &self.global_endpoints[index]
    }

    fn select_random_sleep(&self) -> u64 {
        let mut rng = rand::thread_rng();
        rng.gen_range(0..self.sleep)
    }

    pub fn get_endpoints(&self) -> &Arc<RwLock<HashMap<String, Results>>> {
        &self.endpoints
    }

    fn add_endpoint_response_time(&self, response_time: u32, endpoint: &EndPoint) {
        endpoint.add_response_time(response_time);
        self.endpoints
            .write()
            .entry(endpoint.url.clone())
            .or_insert(Results::new())
            .add_response_time(response_time);
        self.add_response_time(response_time);
    }
}

impl Drop for User {
    fn drop(&mut self) {
        self.token.lock().unwrap().cancel(); //stop main thread
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

    fn get_results(&self) -> Arc<RwLock<Results>> {
        self.results.clone()
    }
}
