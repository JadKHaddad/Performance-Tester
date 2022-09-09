use crate::{user::User, EndPoint, Results, Status, Updatble};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use std::time::Instant;
use tokio::select;
use tokio::time::error::Elapsed;
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;

#[derive(Clone, Debug)]
pub struct Test {
    status: Arc<RwLock<Status>>,
    token: Arc<Mutex<CancellationToken>>,
    background_token: Arc<Mutex<CancellationToken>>,
    user_count: u32,
    run_time: Option<u64>,
    sleep: u64,
    host: Arc<String>,
    endpoints: Arc<Vec<EndPoint>>,
    global_headers: Option<HashMap<String, String>>,
    results: Arc<RwLock<Results>>,
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
            background_token: Arc::new(Mutex::new(CancellationToken::new())),
            user_count,
            run_time,
            sleep,
            host: Arc::new(host),
            endpoints: Arc::new(endpoints),
            global_headers,
            results: Arc::new(RwLock::new(Results::new())),
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

    async fn select_run_mode_and_run(&mut self) -> Result<(), Elapsed> {
        self.set_start_timestamp(Instant::now());
        self.set_status(Status::RUNNING);
        let background_test = self.clone();
        tokio::spawn(async move {
            background_test.run_update_in_background(2).await;
        });
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
                println!("test cancelled");
                self.set_end_timestamp(Instant::now());
                self.set_status(Status::STOPPED);
                self.stop_users();
                self.background_token.lock().unwrap().cancel();
            }
            _ = self.select_run_mode_and_run() => {
                self.set_end_timestamp(Instant::now());
                self.set_status(Status::FINISHED);
                self.finish_users();
                self.background_token.lock().unwrap().cancel();
            }
        }
    }

    fn set_start_timestamp(&self, start_timestamp: Instant) {
        *self.start_timestamp.write() = Some(start_timestamp);
    }

    fn set_end_timestamp(&self, end_timestamp: Instant) {
        *self.end_timestamp.write() = Some(end_timestamp);
    }

    fn set_status(&self, status: Status) {
        *self.status.write() = status;
    }

    async fn run_update_in_background(&self, thread_sleep_time: u64) {
        let test_handler = self.clone();
        let background_token = self.background_token.lock().unwrap().clone();
        select! {
            _ = background_token.cancelled() => {
                println!("test update in background cancelled");
            }
            _ = tokio::spawn(async move {
                loop {
                    println!("updating");
                    if let Some(elapsed) = Test::calculate_elapsed_time(
                        *test_handler.start_timestamp.read(),
                        *test_handler.end_timestamp.read(),
                    ) {
                        test_handler.calculate_requests_per_second(&elapsed);
                    }
                    tokio::time::sleep(Duration::from_secs(thread_sleep_time)).await;
                }
            }) => {

            }
        }
    }

    async fn run_with_timeout(&mut self, time_out: u64) -> Result<(), Elapsed> {
        let future = self.run_forever();
        timeout(Duration::from_secs(time_out), future).await
        
    }

    async fn run_forever(&mut self) {
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

    pub fn stop(&self) {
        self.token.lock().unwrap().cancel();
    }

    pub fn stop_a_user(&self, user_id: usize) -> Result<(), String> {
        match self.users.read().get(user_id) {
            Some(user) => {
                user.stop();
                Ok(())
            }
            None => Err(String::from("user not found")),
        }
    }

    pub fn stop_users(&self) {
        for user in self.users.read().iter() {
            user.stop();
            user.set_status_with_check(Status::STOPPED);
        }
    }

    pub fn finish_users(&self) {
        for user in self.users.read().iter() {
            user.stop();
            user.set_status_with_check(Status::FINISHED);
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

    pub fn get_users(&self) -> &Arc<RwLock<Vec<User>>> {
        &self.users
    }

    pub fn get_endpoints(&self) -> &Arc<Vec<EndPoint>> {
        &self.endpoints
    }

    pub fn get_status(&self) -> &Arc<RwLock<Status>> {
        &self.status
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

    fn get_results(&self) -> Arc<RwLock<Results>> {
        self.results.clone()
    }
}
