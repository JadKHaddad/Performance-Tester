use rand::Rng;
use reqwest::Client;
use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use std::time::Instant;
use tokio::time::error::Elapsed;
use tokio::time::timeout;
//use parking_lot::RwLock;
use tokio_util::sync::CancellationToken;
use tokio::select;

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
pub struct EndPoint {
    method: Method,
    url: String,
    total_requests: Arc<Mutex<u32>>,
}

impl EndPoint {
    pub fn new(method: Method, url: String) -> EndPoint {
        EndPoint {
            method,
            url,
            total_requests: Arc::new(Mutex::new(0)),
        }
    }

    pub fn get_method(&self) -> &Method {
        &self.method
    }

    pub fn get_url(&self) -> &String {
        &self.url
    }
}

impl fmt::Display for EndPoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} | {} | {}",
            self.method,
            self.url,
            self.total_requests.lock().unwrap()
        )
    }
}
#[derive(Clone, Debug)]
pub struct TestData {

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
    total_requests: Arc<Mutex<u32>>,
    total_response_time: Arc<Mutex<u32>>,
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
            total_requests: Arc::new(Mutex::new(0)),
            total_response_time: Arc::new(Mutex::new(0)),
        }
    }

    pub async fn select_run_mode_and_run(&self) -> Result<(), Elapsed> {
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

    pub async fn run(self) -> Self {
        
        let token = self.token.lock().unwrap().clone();
        
        let join_handle = tokio::spawn(async move {
            
            select! {
                _ = token.cancelled() => {
                    // The token was cancelled
                    println!("canceled");
                    self
                }
                _ = self.select_run_mode_and_run() => {
                    self
                }
            }
        });
        //let token = token.lock().unwrap().clone();
        join_handle.await.unwrap()
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

                        let mut total_response_time = test.total_response_time.lock().unwrap();
                        *total_response_time += duration.as_millis() as u32;
                    }
                    {
                        let mut total_requests = test.total_requests.lock().unwrap();
                        *total_requests += 1;

                        let mut end_point_total_requests = end_point.total_requests.lock().unwrap();
                        *end_point_total_requests += 1;
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

    pub fn stop(&self){

        println!("canceling");
        self.token.lock().unwrap().cancel();
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

    let test_handle = test.clone();
    tokio::spawn(async move {
        println!("canceling test in 5 seconds");
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        println!("attemting cancel");
        test_handle.stop();
    });
    
    let test = test.run().await;
    let total_requests = test.total_requests.lock().unwrap();
    println!("total requests: {}", total_requests);
    println!("total requests per end point:");
    for end_point in &test.end_points {
        println!("{}", end_point);
    }


}
