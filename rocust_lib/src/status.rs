use serde::{Deserialize, Serialize};
use std::fmt;

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
