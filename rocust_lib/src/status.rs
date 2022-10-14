use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum Status {
    Created,
    Connected,
    Running,
    Stopped,
    Finished,
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Status::Created => write!(f, "CREATED"),
            Status::Connected => write!(f, "CONNECTED"),
            Status::Running => write!(f, "RUNNING"),
            Status::Stopped => write!(f, "STOPPED"),
            Status::Finished => write!(f, "FINISHED"),
        }
    }
}
