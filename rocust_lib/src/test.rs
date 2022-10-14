use crate::{EndPoint, HasResults, LogType, Logger, Results, Runnable, Status};
use async_trait::async_trait;
use parking_lot::RwLock;
use prettytable::{row, Table};
use serde::{
    de::{MapAccess, Visitor},
    ser::SerializeStruct,
    Deserialize, Deserializer, Serialize, Serializer,
};
use std::{
    collections::HashMap,
    error::Error,
    fmt,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::{fs, select};
use tokio_util::sync::CancellationToken;

use user::User;
pub mod user;

#[derive(Clone, Debug)]
pub struct Test {
    id: String,
    status: Arc<RwLock<Status>>,
    background_token: Arc<Mutex<CancellationToken>>, //
    user_count: u32,
    run_time: Option<u64>,
    sleep: (u64, u64),
    host: Arc<String>,
    endpoints: Arc<Vec<EndPoint>>,
    global_headers: Arc<Option<HashMap<String, String>>>,
    results: Arc<RwLock<Results>>,
    start_timestamp: Arc<RwLock<Option<Instant>>>, //
    end_timestamp: Arc<RwLock<Option<Instant>>>,   //
    users: Arc<RwLock<Vec<User>>>,
    logger: Arc<Logger>,
    print_stats_to_console: Arc<bool>,
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
        print_log_to_console: bool,
        print_stats_to_console: bool,
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
            global_headers: Arc::new(global_headers),
            results: Arc::new(RwLock::new(Results::new())),
            start_timestamp: Arc::new(RwLock::new(None)),
            end_timestamp: Arc::new(RwLock::new(None)),
            users: Arc::new(RwLock::new(Vec::new())),
            logger: Arc::new(Logger::new(logfile_path, print_log_to_console)),
            print_stats_to_console: Arc::new(print_stats_to_console),
        }
    }

    pub fn from_json(json: &str) -> Result<Self, Box<dyn Error>> {
        let test: Self = serde_json::from_str(json)?;
        Ok(test)
    }

    pub async fn from_file(path: &str) -> Result<Self, Box<dyn Error>> {
        let json = fs::read_to_string(path).await?;
        let test: Self = Self::from_json(&json)?;
        Ok(test)
    }

    pub fn into_json(&self) -> Result<String, Box<dyn Error>> {
        let json = serde_json::to_string(self)?;
        Ok(json)
    }

    pub async fn into_file(&self, path: &str) -> Result<(), Box<dyn Error>> {
        let json = self.into_json()?;
        fs::write(path, json).await?;
        Ok(())
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
            if *self.print_stats_to_console {
                self.print_stats();
            }
            //log
            let _ = self.logger.flush_buffer().await;
            tokio::time::sleep(Duration::from_secs(thread_sleep_time)).await;
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

    pub fn set_print_stats_to_console(&mut self, print_stats_to_console: bool) {
        self.print_stats_to_console = Arc::new(print_stats_to_console);
    }

    pub fn set_start_timestamp(&self, start_timestamp: Instant) {
        *self.start_timestamp.write() = Some(start_timestamp);
    }

    pub fn set_end_timestamp(&self, end_timestamp: Instant) {
        *self.end_timestamp.write() = Some(end_timestamp);
    }

    fn set_status(&self, status: Status) {
        *self.status.write() = status;
    }

    pub fn set_logger(&mut self, logger: Arc<Logger>) {
        self.logger = logger;
    }

    pub fn set_run_time(&mut self, run_time: Option<u64>) {
        self.run_time = run_time;
    }

    pub fn print_stats(&self) {
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

    pub fn calculate_elapsed_time(
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

    pub fn get_run_time(&self) -> &Option<u64> {
        &self.run_time
    }

    pub fn get_start_timestamp(&self) -> &Arc<RwLock<Option<Instant>>> {
        &self.start_timestamp
    }

    pub fn get_end_timestamp(&self) -> &Arc<RwLock<Option<Instant>>> {
        &self.end_timestamp
    }
}

#[async_trait]
impl Runnable for Test {
    async fn run(&mut self) {
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

    fn stop(&self) {
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

    fn get_status(&self) -> Status {
        let status = &*self.status.read();
        status.clone()
    }

    fn get_id(&self) -> &String {
        &self.id
    }
}

impl fmt::Display for Test {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // let global_headers = if let Some(s_global_headers) = &*self.global_headers {
        //     format!("{:?}", s_global_headers)
        // } else {
        //     String::from("<>")
        // };
        let global_headers = if self.global_headers.is_some() {
            &*self.global_headers
        } else {
            &None
        };

        write!(
            f,
            "Status [{}] | Users [{}] | RunTime [{}] | Sleep [{} - {}] | Host [{}] | GlobalHeaders [{:?}] | Results [{}] | StartTimestamp [{:?}] | EndTimestamp [{:?}] | ElapsedTime [{:?}]",
            self.status.read(),
            self.user_count,
            self.run_time.unwrap_or(0),
            self.sleep.0,
            self.sleep.1,
            self.host,
            global_headers,
            self.results.read(),
            self.start_timestamp,
            self.end_timestamp,
            self.get_elapsed_time()
        )
    }
}

impl HasResults for Test {
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

impl Serialize for Test {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("Test", 12)?;
        state.serialize_field("id", &self.id)?;
        state.serialize_field("status", &*self.status.read())?;
        state.serialize_field("user_count", &self.user_count)?;
        state.serialize_field("run_time", &self.run_time)?;
        state.serialize_field("sleep", &self.sleep)?;
        state.serialize_field("host", &*self.host)?;
        state.serialize_field("endpoints", &*self.endpoints)?;
        state.serialize_field("global_headers", &*self.global_headers)?;
        state.serialize_field("results", &*self.results.read())?;
        state.serialize_field("users", &*self.users.read())?;
        state.serialize_field("logger", &*self.logger)?;
        state.serialize_field("print_stats_to_console", &*self.print_stats_to_console)?;
        state.end()
    }
}

impl<'de> Deserialize<'de> for Test {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct TestVisitor;

        #[derive(Deserialize)]
        #[serde(field_identifier, rename_all = "snake_case")]
        enum Field {
            Id,
            Status,
            UserCount,
            RunTime,
            Sleep,
            Host,
            Endpoints,
            GlobalHeaders,
            Results,
            Users,
            Logger,
            PrintStatsToConsole,
        }
        impl<'de> Visitor<'de> for TestVisitor {
            type Value = Test;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("struct Test")
            }

            fn visit_map<V>(self, mut map: V) -> Result<Test, V::Error>
            where
                V: MapAccess<'de>,
            {
                let mut id: Option<String> = None;
                let mut status: Option<Status> = None;
                let mut user_count: Option<u32> = None;
                let mut run_time: Option<Option<u64>> = None;
                let mut sleep: Option<(u64, u64)> = None;
                let mut host: Option<String> = None;
                let mut endpoints: Option<Vec<EndPoint>> = None;
                let mut global_headers: Option<Option<HashMap<String, String>>> = None;
                let mut results: Option<Results> = None;
                let mut users: Option<Vec<User>> = None;
                let mut logger: Option<Logger> = None;
                let mut print_stats_to_console: Option<bool> = None;

                while let Some(key) = map.next_key()? {
                    match key {
                        Field::Id => {
                            if id.is_some() {
                                return Err(serde::de::Error::duplicate_field("id"));
                            }
                            id = Some(map.next_value()?);
                        }
                        Field::Status => {
                            if status.is_some() {
                                return Err(serde::de::Error::duplicate_field("status"));
                            }
                            status = Some(map.next_value()?);
                        }
                        Field::UserCount => {
                            if user_count.is_some() {
                                return Err(serde::de::Error::duplicate_field("user_count"));
                            }
                            user_count = Some(map.next_value()?);
                        }
                        Field::RunTime => {
                            if run_time.is_some() {
                                return Err(serde::de::Error::duplicate_field("run_time"));
                            }
                            run_time = Some(map.next_value()?);
                        }
                        Field::Sleep => {
                            if sleep.is_some() {
                                return Err(serde::de::Error::duplicate_field("sleep"));
                            }
                            sleep = Some(map.next_value()?);
                        }
                        Field::Host => {
                            if host.is_some() {
                                return Err(serde::de::Error::duplicate_field("host"));
                            }
                            host = Some(map.next_value()?);
                        }
                        Field::Endpoints => {
                            if endpoints.is_some() {
                                return Err(serde::de::Error::duplicate_field("endpoints"));
                            }
                            endpoints = Some(map.next_value()?);
                        }
                        Field::GlobalHeaders => {
                            if global_headers.is_some() {
                                return Err(serde::de::Error::duplicate_field("global_headers"));
                            }
                            global_headers = Some(map.next_value()?);
                        }
                        Field::Results => {
                            if results.is_some() {
                                return Err(serde::de::Error::duplicate_field("results"));
                            }
                            results = Some(map.next_value()?);
                        }
                        Field::Users => {
                            if users.is_some() {
                                return Err(serde::de::Error::duplicate_field("users"));
                            }
                            users = Some(map.next_value()?);
                        }
                        Field::Logger => {
                            if logger.is_some() {
                                return Err(serde::de::Error::duplicate_field("logger"));
                            }
                            logger = Some(map.next_value()?);
                        }
                        Field::PrintStatsToConsole => {
                            if print_stats_to_console.is_some() {
                                return Err(serde::de::Error::duplicate_field(
                                    "print_stats_to_console",
                                ));
                            }
                            print_stats_to_console = Some(map.next_value()?);
                        }
                    }
                }

                let id = id.ok_or_else(|| serde::de::Error::missing_field("id"))?;
                let status = status.ok_or_else(|| serde::de::Error::missing_field("status"))?;
                let user_count =
                    user_count.ok_or_else(|| serde::de::Error::missing_field("user_count"))?;
                let run_time =
                    run_time.ok_or_else(|| serde::de::Error::missing_field("run_time"))?;
                let sleep = sleep.ok_or_else(|| serde::de::Error::missing_field("sleep"))?;
                let host = host.ok_or_else(|| serde::de::Error::missing_field("host"))?;
                let endpoints =
                    endpoints.ok_or_else(|| serde::de::Error::missing_field("endpoints"))?;
                let global_headers = global_headers
                    .ok_or_else(|| serde::de::Error::missing_field("global_headers"))?;
                let results = results.ok_or_else(|| serde::de::Error::missing_field("results"))?;
                let users = users.ok_or_else(|| serde::de::Error::missing_field("users"))?;
                let logger = logger.ok_or_else(|| serde::de::Error::missing_field("logger"))?;
                let print_stats_to_console = print_stats_to_console
                    .ok_or_else(|| serde::de::Error::missing_field("print_stats_to_console"))?;

                Ok(Test {
                    id,
                    status: Arc::new(RwLock::new(status)),
                    background_token: Arc::new(Mutex::new(CancellationToken::new())),
                    user_count,
                    run_time,
                    sleep,
                    host: Arc::new(host),
                    endpoints: Arc::new(endpoints),
                    global_headers: Arc::new(global_headers),
                    results: Arc::new(RwLock::new(results)),
                    start_timestamp: Arc::new(RwLock::new(None)),
                    end_timestamp: Arc::new(RwLock::new(None)),
                    users: Arc::new(RwLock::new(users)),
                    logger: Arc::new(logger),
                    print_stats_to_console: Arc::new(print_stats_to_console),
                })
            }
        }
        const FIELDS: &'static [&'static str] = &[
            "id",
            "status",
            "user_count",
            "run_time",
            "sleep",
            "host",
            "endpoints",
            "global_headers",
            "results",
            "users",
            "logger",
        ];
        deserializer.deserialize_struct("Test", &FIELDS, TestVisitor)
    }
}

//might need
/*
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestInfo {
    pub id: String,
    pub user_count: u32,
    pub run_time: Option<u64>,
    pub sleep: (u64, u64),
    pub host: String,
    pub endpoints: Vec<EndPoint>,
    pub global_headers: Option<HashMap<String, String>>,
    pub logfile_path: String,
}

impl TestInfo {
    pub fn new(
        id: String,
        user_count: u32,
        run_time: Option<u64>,
        sleep: (u64, u64),
        host: String,
        endpoints: Vec<EndPoint>,
        global_headers: Option<HashMap<String, String>>,
        logfile_path: String,
    ) -> TestInfo {
        TestInfo {
            id,
            user_count,
            run_time,
            sleep,
            host,
            endpoints,
            global_headers,
            logfile_path
        }
    }

    pub fn into_test(self) -> Test {
        Test::new(
            self.id,
            self.user_count,
            self.run_time,
            self.sleep,
            self.host,
            self.endpoints,
            self.global_headers,
            self.logfile_path,
        )
    }

    pub fn from_json(json: &str) -> Result<Self, Box<dyn Error>> { // TODO repeated code // use a macro
        let test: Self =  serde_json::from_str(json)?;
        Ok(test)
    }

    pub async fn from_file(path: &str) -> Result<Self, Box<dyn Error>> { // TODO repeated code // use a macro
        let json = fs::read_to_string(path).await?;
        let test: Self = Self::from_json(&json)?;
        Ok(test)
    }

    pub fn into_json(&self) -> Result<String, Box<dyn Error>> { // TODO repeated code // use a macro
        let json = serde_json::to_string(self)?;
        Ok(json)
    }

    pub async fn into_file(&self, path: &str) -> Result<(), Box<dyn Error>> { // TODO repeated code // use a macro
        let json = self.into_json()?;
        fs::write(path, json).await?;
        Ok(())
    }
}
*/
