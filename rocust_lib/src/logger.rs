use chrono::{
    format::{DelayedFormat, StrftimeItems},
    DateTime, Utc,
};
use parking_lot::RwLock;
use serde::{
    de::{MapAccess, Visitor},
    ser::SerializeStruct,
    Deserialize, Deserializer, Serialize, Serializer,
};
use std::{error::Error, fmt, sync::Arc};
use tokio::{fs, fs::OpenOptions, io::AsyncWriteExt};

pub enum LogType {
    Info,
    Debug,
    Error,
    Warning,
    Critical
}

impl fmt::Display for LogType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            LogType::Info => write!(f, "INFO"),
            LogType::Debug => write!(f, "DEBUG"),
            LogType::Error => write!(f, "ERROR"),
            LogType::Warning => write!(f, "WARNING"),
            LogType::Critical => write!(f, "CRITICAL"),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Logger {
    logfile_path: String,
    buffer: Arc<RwLock<Vec<String>>>,
    print_to_console: bool,
}

//TODO: Create log folder if does not exist on logger creation or use logger init,
//  cuz tokio::fs::create_dir_all is async and we need to wait for it to finish
impl Logger {
    pub fn new(logfile_path: String, print_to_console: bool) -> Logger {
        Logger {
            logfile_path,
            buffer: Arc::new(RwLock::new(Vec::new())),
            print_to_console,
        }
    }

    pub fn set_print_to_console(&mut self, print_to_console: bool) {
        self.print_to_console = print_to_console;
    }

    fn get_date_and_time(&self) -> DelayedFormat<StrftimeItems> {
        let now: DateTime<Utc> = Utc::now();
        now.format("%Y.%m.%d %H:%M:%S")
    }

    fn format_message(&self, log_type: &LogType, message: &str) -> String {
        format!("{} {} {}", self.get_date_and_time(), log_type, message)
    }

    pub fn log_buffered(&self, log_type: LogType, message: &str) {
        let msg = self.format_message(&log_type, message);
        if self.print_to_console {
            if let LogType::Error = log_type {
                eprintln!("{}", msg);
            } else {
                println!("{}", msg);
            }
        }
        self.buffer.write().push(msg);
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
        if file.write(result.as_bytes()).await.is_err() {
            let message = format!("Failed to write to log file: [{}]", self.logfile_path);
            eprintln!("{}", message);
            return Err(message.into());
        }
        Ok(())
    }

    pub async fn log(&self, log_type: LogType, message: &str) -> Result<(), Box<dyn Error>> {
        let msg = self.format_message(&log_type, message);
        if self.print_to_console {
            if let LogType::Error = log_type {
                eprintln!("{}", msg);
            } else {
                println!("{}", msg);
            }
        }
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .append(true)
            .open(&self.logfile_path)
            .await?;
        if file.write(format!("{}\n", msg).as_bytes()).await.is_err() {
            let message = format!("Failed to write to log file: [{}]", self.logfile_path);
            eprintln!("{}", message);
            return Err(message.into());
        }
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
                    print_to_console: false,
                })
            }
        }
        const FIELDS: &'static [&'static str] = &["logfile_path"];
        deserializer.deserialize_struct("Logger", &FIELDS, LoggerVisitor)
    }
}
