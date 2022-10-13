use crate::{Results, Status};
use async_trait::async_trait;
use parking_lot::RwLock;
use std::{sync::Arc, time::Duration};

pub trait HasResults {
    fn add_response_time(&self, response_time: u32);
    fn add_failed(&self);
    fn add_connection_error(&self);
    fn set_requests_per_second(&self, requests_per_second: f64);
    fn calculate_requests_per_second(&self, elapsed: &Duration);
    fn calculate_failed_requests_per_second(&self, elapsed: &Duration);
    fn get_results(&self) -> Arc<RwLock<Results>>;
}

#[async_trait]
pub trait Runnable {
    async fn run(&mut self);
    fn stop(&self);
    fn finish(&self);
    fn get_status(&self) -> Status;
    fn get_id(&self) -> &String;
}
