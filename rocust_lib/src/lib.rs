extern crate prettytable;
use chrono::format::{DelayedFormat, StrftimeItems};
use chrono::{DateTime, Utc};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::error::Error;
use std::fmt;
use std::sync::Arc;
use std::time::Duration;
use tokio::fs::OpenOptions;
use tokio::io::AsyncWriteExt;
pub mod test;

pub enum LogType {
    INFO,
    DEBUG,
    ERROR,
    TRACE,
}

impl fmt::Display for LogType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            LogType::INFO => write!(f, "INFO"),
            LogType::DEBUG => write!(f, "DEBUG"),
            LogType::ERROR => write!(f, "ERROR"),
            LogType::TRACE => write!(f, "TRACE"),
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

    pub fn log_buffed(&self, log_type: LogType, message: &str) {
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

trait Updatble {
    fn add_response_time(&self, response_time: u32);
    fn set_requests_per_second(&self, requests_per_second: f64);
    fn calculate_requests_per_second(&self, elapsed: &Duration);
    fn get_results(&self) -> Arc<RwLock<Results>>;
}

#[derive(Clone, Debug)]
pub struct Results {
    total_requests: u32,
    total_response_time: u32,
    average_response_time: u32,
    requests_per_second: f64,
}

impl Results {
    pub fn new() -> Results {
        Results {
            total_requests: 0,
            total_response_time: 0,
            average_response_time: 0,
            requests_per_second: 0.0,
        }
    }

    pub fn add_response_time(&mut self, response_time: u32) {
        self.total_response_time += response_time;
        self.total_requests += 1;
        self.average_response_time = self.total_response_time / self.total_requests;
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

impl fmt::Display for Results {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Total Requests [{}] | Requests per Second [{}] | Total Response Time [{}] | Average Response Time [{}]",
            self.total_requests, self.requests_per_second, self.total_response_time, self.average_response_time
        )
    }
}

#[derive(Clone, Debug)]
pub struct EndPoint {
    method: Method,
    url: String,
    results: Arc<RwLock<Results>>,
    headers: Option<HashMap<String, String>>,
}

impl EndPoint {
    pub fn new(method: Method, url: String, headers: Option<HashMap<String, String>>) -> EndPoint {
        EndPoint {
            method,
            url,
            results: Arc::new(RwLock::new(Results::new())),
            headers,
        }
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

    fn get_results(&self) -> Arc<RwLock<Results>> {
        self.results.clone()
    }
}
