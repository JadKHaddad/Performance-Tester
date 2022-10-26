use serde::{Deserialize, Serialize};
use std::{fmt, time::Duration};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SentResults {
    pub total_requests: u32,
    pub total_failed_requests: u32,
    pub total_connection_errors: u32,
    pub total_response_time: u32,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Results {
    pub total_requests: u32,
    pub total_failed_requests: u32,
    pub total_connection_errors: u32,
    pub total_response_time: u32,
    pub average_response_time: u32,
    pub min_response_time: u32,
    pub median_response_time: u32,
    pub max_response_time: u32,
    pub requests_per_second: f64,
    pub failed_requests_per_second: f64,
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

    pub fn create_sent_results(&self) -> SentResults {
        SentResults {
            total_requests: self.total_requests,
            total_failed_requests: self.total_failed_requests,
            total_connection_errors: self.total_connection_errors,
            total_response_time: self.total_response_time,
        }
    }

    pub fn add_response_time(&mut self, response_time: u32) {
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

    pub fn add_failed(&mut self) {
        self.total_requests += 1;
        self.total_failed_requests += 1;
    }

    pub fn add_connection_error(&mut self) {
        self.total_connection_errors += 1;
    }

    pub fn get_total_requests(&self) -> u32 {
        self.total_requests
    }

    pub fn get_total_failed_requests(&self) -> u32 {
        self.total_failed_requests
    }

    pub fn set_requests_per_second(&mut self, requests_per_second: f64) {
        self.requests_per_second = requests_per_second;
    }

    fn set_failed_requests_per_second(&mut self, failed_requests_per_second: f64) {
        self.failed_requests_per_second = failed_requests_per_second;
    }

    pub fn calculate_requests_per_second(&mut self, elapsed: &Duration) {
        let total_requests = self.get_total_requests();
        let requests_per_second = total_requests as f64 / elapsed.as_secs_f64();
        self.set_requests_per_second(requests_per_second);
    }

    pub fn calculate_failed_requests_per_second(&mut self, elapsed: &Duration) {
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
