extern crate prettytable;
use chrono::format::{DelayedFormat, StrftimeItems};
use chrono::{DateTime, Utc};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::error::Error;
use std::fmt;
use std::sync::Arc;
use std::time::Duration;
use tokio::fs::OpenOptions;
use tokio::io::AsyncWriteExt;

pub mod master;
pub mod test;
pub mod worker;
pub enum LogType {
    INFO,
    DEBUG,
    ERROR,
    TRACE,
    WARNING,
}

impl fmt::Display for LogType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            LogType::INFO => write!(f, "INFO"),
            LogType::DEBUG => write!(f, "DEBUG"),
            LogType::ERROR => write!(f, "ERROR"),
            LogType::TRACE => write!(f, "TRACE"),
            LogType::WARNING => write!(f, "WARNING"),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Logger {
    logfile_path: String,
    buffer: Arc<RwLock<Vec<String>>>,
}

impl Logger {
    pub fn new(logfile_path: String) -> Logger {
        Logger {
            logfile_path,
            buffer: Arc::new(RwLock::new(Vec::new())),
        }
    }

    fn get_date_and_time(&self) -> DelayedFormat<StrftimeItems> {
        let now: DateTime<Utc> = Utc::now();
        now.format("%Y.%m.%d %H:%M:%S")
    }

    fn format_message(&self, log_type: LogType, message: &str) -> String {
        format!("{} {} {}", self.get_date_and_time(), log_type, message)
    }

    pub fn log_buffered(&self, log_type: LogType, message: &str) {
        self.buffer
            .write()
            .push(self.format_message(log_type, message));
    }

    pub async fn flush_buffer(&self) -> Result<(), Box<dyn Error>> {
        let mut result = String::new();
        {
            let mut buffer = self.buffer.write();
            for message in buffer.iter() {
                result.push_str(message);
                result.push_str("\n");
            }
            buffer.clear();
        }

        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .append(true)
            .open(&self.logfile_path)
            .await?;
        file.write(result.as_bytes()).await?;
        Ok(())
    }

    pub async fn log(&self, log_type: LogType, message: &str) -> Result<(), Box<dyn Error>> {
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .append(true)
            .open(&self.logfile_path)
            .await?;

        file.write(self.format_message(log_type, message).as_bytes())
            .await?;
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
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

#[derive(Clone, Debug, Deserialize, Serialize)]
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

trait Updatble {
    fn add_response_time(&self, response_time: u32);
    fn add_failed(&self);
    fn add_connection_error(&self);
    fn set_requests_per_second(&self, requests_per_second: f64);
    fn calculate_requests_per_second(&self, elapsed: &Duration);
    fn calculate_failed_requests_per_second(&self, elapsed: &Duration);
    fn get_results(&self) -> Arc<RwLock<Results>>;
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Results {
    total_requests: u32,
    total_failed_requests: u32,
    total_connection_errors: u32,
    total_response_time: u32,
    average_response_time: u32,
    min_response_time: u32,
    median_response_time: u32,
    max_response_time: u32,
    requests_per_second: f64,
    failed_requests_per_second: f64,
}

impl Results {
    pub fn new() -> Results {
        Results {
            total_requests: 0,
            total_failed_requests: 0,
            total_connection_errors: 0,
            total_response_time: 0,
            average_response_time: 0,
            min_response_time: 0,
            median_response_time: 0,
            max_response_time: 0,
            requests_per_second: 0.0,
            failed_requests_per_second: 0.0,
        }
    }

    fn add_response_time(&mut self, response_time: u32) {
        self.total_response_time += response_time;
        self.total_requests += 1;
        self.average_response_time = self.total_response_time / self.total_requests;
        if self.min_response_time == 0 || response_time < self.min_response_time {
            self.min_response_time = response_time;
        }
        if response_time > self.max_response_time {
            self.max_response_time = response_time;
        }
    }

    fn add_failed(&mut self) {
        self.total_requests += 1;
        self.total_failed_requests += 1;
    }

    fn add_connection_error(&mut self) {
        self.total_connection_errors += 1;
    }

    fn get_total_requests(&self) -> u32 {
        self.total_requests
    }

    fn get_total_failed_requests(&self) -> u32 {
        self.total_failed_requests
    }

    fn set_requests_per_second(&mut self, requests_per_second: f64) {
        self.requests_per_second = requests_per_second;
    }

    fn set_failed_requests_per_second(&mut self, failed_requests_per_second: f64) {
        self.failed_requests_per_second = failed_requests_per_second;
    }

    fn calculate_requests_per_second(&mut self, elapsed: &Duration) {
        let total_requests = self.get_total_requests();
        let requests_per_second = total_requests as f64 / elapsed.as_secs_f64();
        self.set_requests_per_second(requests_per_second);
    }

    fn calculate_failed_requests_per_second(&mut self, elapsed: &Duration) {
        let total_failed_requests = self.get_total_failed_requests();
        let failed_requests_per_second = total_failed_requests as f64 / elapsed.as_secs_f64();
        self.set_failed_requests_per_second(failed_requests_per_second);
    }
}

impl fmt::Display for Results {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Total Requests [{}] | Requests per Second [{}] | Total Response Time [{}] | Average Response Time [{}]",
            self.total_requests, self.requests_per_second, self.total_response_time, self.average_response_time
        )
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SerDeserEndpoint {
    pub method: Method,
    pub url: String,
    pub headers: Option<HashMap<String, String>>,
    pub params: Option<Vec<(String, String)>>,
    pub body: Option<String>,
}

impl SerDeserEndpoint {
    //TODO
    pub fn into_endpoint(self) -> EndPoint {
        EndPoint::new(self.method, self.url, self.headers, self.params, self.body)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct EndPoint {
    method: Method,
    url: String,
    headers: Option<HashMap<String, String>>,
    params: Option<Vec<(String, String)>>,
    body: Option<String>,
    #[serde(with = "serde_rw_lock")]
    results: Arc<RwLock<Results>>,
}

impl EndPoint {
    fn new(
        method: Method,
        url: String,
        headers: Option<HashMap<String, String>>,
        params: Option<Vec<(String, String)>>,
        body: Option<String>,
    ) -> EndPoint {
        EndPoint {
            method,
            url,
            params,
            body,
            results: Arc::new(RwLock::new(Results::new())),
            headers,
        }
    }

    pub fn into_serdeserendpoint(self) -> SerDeserEndpoint {
        todo!()
    }

    pub fn new_get(
        url: String,
        headers: Option<HashMap<String, String>>,
        params: Option<Vec<(String, String)>>,
    ) -> EndPoint {
        EndPoint::new(Method::GET, url, headers, params, None)
    }

    pub fn new_post(
        url: String,
        headers: Option<HashMap<String, String>>,
        body: Option<String>,
    ) -> EndPoint {
        EndPoint::new(Method::POST, url, headers, None, body)
    }

    pub fn new_put(
        url: String,
        headers: Option<HashMap<String, String>>,
        body: Option<String>,
    ) -> EndPoint {
        EndPoint::new(Method::PUT, url, headers, None, body)
    }

    pub fn new_delete(url: String, headers: Option<HashMap<String, String>>) -> EndPoint {
        EndPoint::new(Method::DELETE, url, headers, None, None)
    }

    pub fn get_method(&self) -> &Method {
        &self.method
    }

    pub fn get_url(&self) -> &String {
        &self.url
    }

    pub fn get_results(&self) -> &Arc<RwLock<Results>> {
        &self.results
    }

    pub fn get_params(&self) -> &Option<Vec<(String, String)>> {
        &self.params
    }

    pub fn get_body(&self) -> &Option<String> {
        &self.body
    }

    fn add_response_time(&self, response_time: u32) {
        self.results.write().add_response_time(response_time);
    }

    fn add_failed(&self) {
        self.results.write().add_failed();
    }

    fn add_connection_error(&self) {
        self.results.write().add_connection_error();
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

impl Updatble for EndPoint {
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
    }

    fn calculate_failed_requests_per_second(&self, elapsed: &Duration) {
        self.results
            .write()
            .calculate_failed_requests_per_second(elapsed);
    }

    fn get_results(&self) -> Arc<RwLock<Results>> {
        self.results.clone()
    }
}

mod serde_rw_lock {
    use parking_lot::RwLock;
    use serde::{
        Deserialize, Deserializer, Serialize, Serializer,
    };
    use std::sync::Arc;

    pub fn serialize<S, T>(val: &Arc<RwLock<T>>, s: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
        T: Serialize,
    {
        T::serialize(&*val.read(), s)
    }

    pub fn deserialize<'de, D, T>(d: D) -> Result<Arc<RwLock<T>>, D::Error>
    where
        D: Deserializer<'de>,
        T: Deserialize<'de>,
    {
        Ok(Arc::new(RwLock::new(T::deserialize(d)?)))
    }
}

mod serde_mutex_cancalation_token {
    use serde::{
        Deserialize, Deserializer, Serialize, Serializer,
    };
    use tokio_util::sync::CancellationToken;
    use std::sync::{Arc, Mutex};

    pub fn serialize<S, T>(val: &Arc<Mutex<CancellationToken>>, s: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
        T: Serialize,
    {
        String::from("Token").serialize(s)
    }

    pub fn deserialize<'de, D, T>(d: D) -> Result<Arc<Mutex<CancellationToken>>, D::Error>
    where
        D: Deserializer<'de>,
        T: Deserialize<'de>,
    {
        Ok(Arc::new(Mutex::new(CancellationToken::new())))
    }
}

mod serde_arc {
    use serde::{
        Deserialize, Deserializer, Serialize, Serializer,
    };
    use std::sync::Arc;

    pub fn serialize<S, T>(val: &Arc<T>, s: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
        T: Serialize,
    {
        T::serialize(&*val, s)
    }

    pub fn deserialize<'de, D, T>(d: D) -> Result<Arc<T>, D::Error>
    where
        D: Deserializer<'de>,
        T: Deserialize<'de>,
    {
        Ok(Arc::new(T::deserialize(d)?))
    }
}
