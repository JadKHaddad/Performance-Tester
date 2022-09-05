use rand::Rng;
use reqwest::Client;
use std::thread;
use std::time::Duration;
use std::time::Instant;
use tokio::time::error::Elapsed;
use tokio::time::timeout;

#[derive(Clone, Debug)]
pub enum Method {
    GET,
    POST,
    PUT,
    DELETE,
}

#[derive(Clone, Debug)]
pub struct EndPoint {
    method: Method,
    url: String,
}

impl EndPoint {
    pub fn new(method: Method, url: String) -> EndPoint {
        EndPoint { method, url }
    }
    pub fn get_method(&self) -> &Method {
        &self.method
    }
    pub fn get_url(&self) -> &String {
        &self.url
    }
}
#[derive(Clone, Debug)]
pub struct Test {
    client: Client,
    users: u32,
    sleep: u64,
    host: String,
    end_points: Vec<EndPoint>,
    headers: Option<Vec<String>>,
}

impl Test {
    pub fn new(
        users: u32,
        sleep: u64,
        host: String,
        end_points: Vec<EndPoint>,
        headers: Option<Vec<String>>,
    ) -> Self {
        Self {
            client: Client::new(),
            users,
            sleep,
            host,
            end_points,
            headers,
        }
    }

    pub async fn run_with_timeout(&self) -> Result<(), Elapsed> {
        let future = self.run();
        timeout(Duration::from_secs(10), future).await
    }

    pub async fn run(&self) {
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

                    if let Some(headers) = &test.headers {
                        for header in headers {
                            let header = header.split(":").collect::<Vec<&str>>();
                            request = request.header(header[0], header[1]);
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
                    }
                    tokio::time::sleep(std::time::Duration::from_secs(test.select_random_sleep()))
                        .await;
                }
            });
            handles.push(handle);
        }
        println!("all users have been spawned");
        for handle in handles {
            handle.await.unwrap();
        }
    }

    //run with scope, does not support timeout
    pub async fn run_(&self) {
        tokio_scoped::scope(|scope| {
            for i in 0..self.users {
                let user_id = i + 1;
                println!("spwaning user: {}", user_id);
                scope.spawn(async move {
                    loop {
                        let end_point = self.select_random_end_point();
                        let url = format!("{}{}", self.host, end_point.get_url());

                        let mut request = match end_point.get_method() {
                            Method::GET => self.client.get(&url),
                            Method::POST => self.client.post(&url),
                            Method::PUT => self.client.put(&url),
                            Method::DELETE => self.client.delete(&url),
                        };

                        if let Some(headers) = &self.headers {
                            for header in headers {
                                let header = header.split(":").collect::<Vec<&str>>();
                                request = request.header(header[0], header[1]);
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
                        }
                        tokio::time::sleep(std::time::Duration::from_secs(
                            self.select_random_sleep(),
                        ))
                        .await;
                    }
                });
            }
            println!("all users have been spawned");
        });
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
}

#[tokio::main]
async fn main() {
    let test = Test::new(
        9,
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
    match test.run_with_timeout().await {
        Ok(_) => println!("test finished"),
        Err(_) => println!("test timed out"),
    }
}
