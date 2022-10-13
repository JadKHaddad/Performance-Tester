use std::{error::Error, fmt, sync::Arc};
use tokio::{fs::OpenOptions, io::AsyncWriteExt};
use chrono::{format::{DelayedFormat, StrftimeItems}, DateTime, Utc};
use parking_lot::RwLock;
use serde::{
    de::{MapAccess, Visitor},
    ser::SerializeStruct,
    Deserialize, Deserializer, Serialize, Serializer,
};

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
