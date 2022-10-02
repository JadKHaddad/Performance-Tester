use crate::{EndPoint, LogType, Logger, Results, Status, Updatble};
use parking_lot::RwLock;
use prettytable::row;
use prettytable::Table;
use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use std::time::Instant;
use tokio::select;
use tokio_util::sync::CancellationToken;

use user::User;
pub mod user;

#[derive(Clone, Debug)]
pub struct Test {
    id: String,
    status: Arc<RwLock<Status>>,
    background_token: Arc<Mutex<CancellationToken>>,
    user_count: u32,
    run_time: Option<u64>,
    sleep: (u64, u64),
    host: Arc<String>,
    endpoints: Arc<Vec<EndPoint>>,
    global_headers: Option<HashMap<String, String>>,
    results: Arc<RwLock<Results>>,
    start_timestamp: Arc<RwLock<Option<Instant>>>,
    end_timestamp: Arc<RwLock<Option<Instant>>>,
    users: Arc<RwLock<Vec<User>>>,
    logger: Arc<Logger>,
}

impl Test {
    pub fn new(
        id: String,
        user_count: u32,
        run_time: Option<u64>,
        sleep: (u64, u64),
        host: String,
        endpoints: Vec<EndPoint>,
        global_headers: Option<HashMap<String, String>>,
        logfile_path: String,
    ) -> Self {
        Self {
            id,
            status: Arc::new(RwLock::new(Status::CREATED)),
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
            logger: Arc::new(Logger::new(logfile_path)),
        }
    }

    async fn run_update_in_background(&self, thread_sleep_time: u64) {
        let background_token = self.background_token.lock().unwrap().clone();
        select! {
            _ = background_token.cancelled() => {

            }
            _ = self.update_in_background(thread_sleep_time) => {

            }
        }
    }

    async fn update_in_background(&self, thread_sleep_time: u64) {
        loop {
            //calculate requests per second
            if let Some(elapsed) = Test::calculate_elapsed_time(
                *self.start_timestamp.read(),
                *self.end_timestamp.read(),
            ) {
                self.calculate_requests_per_second(&elapsed);
                self.calculate_failed_requests_per_second(&elapsed);
            }
            //print stats
            self.print_stats();
            //log
            let _ = self.logger.flush_buffer().await;
            tokio::time::sleep(Duration::from_secs(thread_sleep_time)).await;
        }
    }

    pub async fn run(&mut self) {
        self.set_start_timestamp(Instant::now());
        self.set_status(Status::RUNNING);
        //run background thread
        let test_handle = self.clone();
        let background_join_handle = tokio::spawn(async move {
            test_handle.run_update_in_background(1).await;
        });
        //set run time
        let mut run_message = String::from("Test running forever, press ctrl+c to stop");
        if let Some(run_time) = self.run_time {
            run_message = format!("Test running for {} seconds", run_time);
            let test_handle = self.clone();
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_secs(run_time)).await;
                test_handle.finish();
                test_handle
                    .logger
                    .log_buffered(LogType::INFO, &format!("Test finished"));
            });
        }
        self.logger.log_buffered(LogType::INFO, &run_message);
        let mut user_join_handles = vec![];
        for i in 0..self.user_count {
            let user_id = i;
            self.logger
                .log_buffered(LogType::INFO, &format!("Spawning user: [{}]", user_id));
            let mut user = self.create_user(user_id.to_string());
            let user_join_handle = tokio::spawn(async move {
                user.run().await;
            });
            user_join_handles.push(user_join_handle);
        }
        self.logger
            .log_buffered(LogType::INFO, &format!("All users have been spawned"));
        for join_handle in user_join_handles {
            match join_handle.await {
                Ok(_) => {}
                Err(e) => {
                    println!("Error while joining user: {}", e);
                    self.logger.log_buffered(LogType::ERROR, &format!("{}", e));
                }
            }
        }
        self.set_end_timestamp(Instant::now());
        self.logger
            .log_buffered(LogType::INFO, &format!("All users have been stopped"));
        //stop background thread
        self.background_token.lock().unwrap().cancel();
        match background_join_handle.await {
            Ok(_) => {}
            Err(e) => {
                self.logger.log_buffered(LogType::ERROR, &format!("{}", e));
            }
        }
        self.logger
            .log_buffered(LogType::INFO, &format!("Background thread stopped"));
        //flush buffer
        let _ = self.logger.flush_buffer().await;
    }

    pub fn create_user(&self, id: String) -> User {
        let user = User::new(
            id,
            self.sleep,
            self.host.clone(),
            self.endpoints.clone(),
            self.global_headers.clone(),
            self.results.clone(),
            self.logger.clone(),
        );
        self.users.write().push(user.clone());
        user
    }

    pub fn stop_a_user(&self, user_id: usize) -> Result<(), String> {
        match self.users.read().get(user_id) {
            Some(user) => {
                user.stop();
                Ok(())
            }
            None => {
                self.logger.log_buffered(
                    LogType::WARNING,
                    &format!(
                        "Attempting to stop a user [{}] that does not exist",
                        user_id
                    ),
                );
                Err(String::from("User not found"))
            }
        }
    }

    pub fn stop(&self) {
        self.set_status(Status::STOPPED);
        for user in self.users.read().iter() {
            user.stop();
        }
    }

    fn finish(&self) {
        self.set_status(Status::FINISHED);
        for user in self.users.read().iter() {
            user.finish();
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

    fn print_stats(&self) {
        let mut table = Table::new();
        table.add_row(row![
            "METH",
            "URL",
            "TOTAL REQ",
            "REQ FAILED",
            "CONN ERR",
            "REQ/S",
            "FAILED REQ/S",
            "TOTAL RES TIME",
            "AVG RES TIME",
            "MIN RES TIME",
            "MAX RES TIME",
        ]);
        for endpoint in self.endpoints.iter() {
            let results = endpoint.get_results().read();
            table.add_row(row![
                endpoint.get_method(),
                endpoint.get_url(),
                results.total_requests,
                results.total_failed_requests,
                results.total_connection_errors,
                results.requests_per_second,
                results.failed_requests_per_second,
                results.total_response_time,
                results.average_response_time,
                results.min_response_time,
                results.max_response_time,
            ]);
        }
        let results = self.results.read();
        table.add_row(row![
            " ",
            "AGR",
            results.total_requests,
            results.total_failed_requests,
            results.total_connection_errors,
            results.requests_per_second,
            results.failed_requests_per_second,
            results.total_response_time,
            results.average_response_time,
            results.min_response_time,
            results.max_response_time,
        ]);
        table.printstd();
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

    pub fn get_id(&self) -> &String {
        &self.id
    }
}

impl fmt::Display for Test {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Status [{}] | Users [{}] | RunTime [{}] | Sleep [{} - {}] | Host [{}] | GlobalHeaders [{:?}] | Results [{}] | StartTimestamp [{:?}] | EndTimestamp [{:?}] | ElapsedTime [{:?}]",
            self.status.read(),
            self.user_count,
            self.run_time.unwrap_or(0),
            self.sleep.0,
            self.sleep.1,
            self.host,
            self.global_headers.as_ref().unwrap_or(&HashMap::new()),
            self.results.read(),
            self.start_timestamp,
            self.end_timestamp,
            self.get_elapsed_time()
        )
    }
}

impl Drop for Test {
    fn drop(&mut self) {
        //println!("Test dropped");
        //self.stop(); //if we stop on drop might be a problem because we might have clones!
    }
}

impl Updatble for Test {
    fn add_response_time(&self, response_time: u32) {
        self.results.write().add_response_time(response_time);
    }

    fn add_failed(&self) {
        self.results.write().add_failed();
    }

    fn add_connection_error(&self) {
        self.results.write().add_connection_error();
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

    fn calculate_failed_requests_per_second(&self, elapsed: &Duration) {
        self.results
            .write()
            .calculate_failed_requests_per_second(elapsed);
        for user in self.users.read().iter() {
            user.calculate_failed_requests_per_second(elapsed);
        }
        for endpoint in self.endpoints.iter() {
            endpoint.calculate_failed_requests_per_second(elapsed);
        }
    }

    fn get_results(&self) -> Arc<RwLock<Results>> {
        self.results.clone()
    }
}
