use crate::{Results, Status};
use async_trait::async_trait;
use parking_lot::RwLock;
use serde::{de::DeserializeOwned, Serialize};
use std::{error::Error, sync::Arc, time::Duration};
use tokio::fs;

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

#[async_trait]
pub trait Jsonable // TODO: use somewhere
where
    Self: Sized + DeserializeOwned + Serialize
{
    fn from_json(json: &str) -> Result<Self, Box<dyn Error>> {
        let res: Self = serde_json::from_str(json)?;
        Ok(res)
    }

    async fn from_file(path: &str) -> Result<Self, Box<dyn Error>> {
        let json = fs::read_to_string(path).await?;
        let res: Self = Jsonable::from_json(&json)?;
        Ok(res)
    }

    fn into_json(&self) -> Result<String, Box<dyn Error>>{
        let json = serde_json::to_string(self)?;
        Ok(json)
    }
    
    async fn into_file(&self, path: &str) -> Result<(), Box<dyn Error>>{
        let json = self.into_json()?;
        fs::write(path, json).await?;
        Ok(())
    }
}
