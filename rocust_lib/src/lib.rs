extern crate prettytable;
use chrono::{format::{DelayedFormat, StrftimeItems}, DateTime, Utc};
use parking_lot::RwLock;
use serde::{
    de::{MapAccess, Visitor},
    ser::SerializeStruct,
    Deserialize, Deserializer, Serialize, Serializer,
};
use std::{collections::HashMap, error::Error, fmt, sync::Arc, time::Duration};
use tokio::{fs::OpenOptions, io::AsyncWriteExt};

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

impl Serialize for Logger {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("Logger", 1)?;
        state.serialize_field("logfile_path", &self.logfile_path)?;
        state.end()
    }
}

impl<'de> Deserialize<'de> for Logger {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct LoggerVisitor;

        #[derive(Deserialize)]
        #[serde(field_identifier, rename_all = "snake_case")]
        enum Field {
            LogfilePath,
        }
        impl<'de> Visitor<'de> for LoggerVisitor {
            type Value = Logger;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("struct Logger")
            }

            fn visit_map<V>(self, mut map: V) -> Result<Logger, V::Error>
            where
                V: MapAccess<'de>,
            {
                let mut logfile_path: Option<String> = None;
                while let Some(key) = map.next_key()? {
                    match key {
                        Field::LogfilePath => {
                            if logfile_path.is_some() {
                                return Err(serde::de::Error::duplicate_field("logfile_path"));
                            }
                            logfile_path = Some(map.next_value()?);
                        }
                    }
                }
                let logfile_path =
                    logfile_path.ok_or_else(|| serde::de::Error::missing_field("logfile_path"))?;

                Ok(Logger {
                    logfile_path,
                    buffer: Arc::new(RwLock::new(Vec::new())),
                })
            }
        }
        const FIELDS: &'static [&'static str] = &["logfile_path"];
        deserializer.deserialize_struct("Logger", &FIELDS, LoggerVisitor)
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
    CONNECTED,
    RUNNING,
    STOPPED,
    FINISHED,
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Status::CREATED => write!(f, "CREATED"),
            Status::CONNECTED => write!(f, "CONNECTED"),
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

#[derive(Clone, Debug)]
pub struct EndPoint {
    method: Method,
    url: String,
    headers: Option<HashMap<String, String>>,
    params: Option<Vec<(String, String)>>,
    body: Option<String>,
    results: Arc<RwLock<Results>>,
}

impl Serialize for EndPoint {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("EndPoint", 6)?;
        state.serialize_field("method", &self.method)?;
        state.serialize_field("url", &self.url)?;
        state.serialize_field("headers", &self.headers)?;
        state.serialize_field("params", &self.params)?;
        state.serialize_field("body", &self.body)?;
        state.serialize_field("results", &*self.results.read())?;
        state.end()
    }
}

impl<'de> Deserialize<'de> for EndPoint {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct EndPointVisitor;

        #[derive(Deserialize)]
        #[serde(field_identifier, rename_all = "snake_case")]
        enum Field {
            Method,
            Url,
            Headers,
            Params,
            Body,
            Results,
        }
        impl<'de> Visitor<'de> for EndPointVisitor {
            type Value = EndPoint;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("struct EndPoint")
            }

            fn visit_map<V>(self, mut map: V) -> Result<EndPoint, V::Error>
            where
                V: MapAccess<'de>,
            {
                let mut method: Option<Method> = None;
                let mut url: Option<String> = None;
                let mut headers: Option<Option<HashMap<String, String>>> = None;
                let mut params: Option<Option<Vec<(String, String)>>> = None;
                let mut body: Option<Option<String>> = None;
                let mut results: Option<Results> = None;

                while let Some(key) = map.next_key()? {
                    match key {
                        Field::Method => {
                            if method.is_some() {
                                return Err(serde::de::Error::duplicate_field("logfile_path"));
                            }
                            method = Some(map.next_value()?);
                        }
                        Field::Url => {
                            if url.is_some() {
                                return Err(serde::de::Error::duplicate_field("logfile_path"));
                            }
                            url = Some(map.next_value()?);
                        }
                        Field::Headers => {
                            if headers.is_some() {
                                return Err(serde::de::Error::duplicate_field("logfile_path"));
                            }
                            headers = Some(map.next_value()?);
                        }
                        Field::Params => {
                            if params.is_some() {
                                return Err(serde::de::Error::duplicate_field("logfile_path"));
                            }
                            params = Some(map.next_value()?);
                        }
                        Field::Body => {
                            if body.is_some() {
                                return Err(serde::de::Error::duplicate_field("logfile_path"));
                            }
                            body = Some(map.next_value()?);
                        }
                        Field::Results => {
                            if results.is_some() {
                                return Err(serde::de::Error::duplicate_field("logfile_path"));
                            }
                            results = Some(map.next_value()?);
                        }
                    }
                }
                let method = method.ok_or_else(|| serde::de::Error::missing_field("method"))?;
                let url = url.ok_or_else(|| serde::de::Error::missing_field("url"))?;
                let headers = headers.ok_or_else(|| serde::de::Error::missing_field("headers"))?;
                let params = params.ok_or_else(|| serde::de::Error::missing_field("params"))?;
                let body = body.ok_or_else(|| serde::de::Error::missing_field("body"))?;
                let results = results.ok_or_else(|| serde::de::Error::missing_field("results"))?;

                Ok(EndPoint {
                    method,
                    url,
                    params,
                    body,
                    results: Arc::new(RwLock::new(results)),
                    headers,
                })
            }
        }
        const FIELDS: &'static [&'static str] =
            &["method", "url", "headers", "params", "body", "results"];
        deserializer.deserialize_struct("EndPoint", &FIELDS, EndPointVisitor)
    }
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
