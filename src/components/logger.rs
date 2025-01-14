use std::sync::Arc;

use chrono::{DateTime, Local};
use lazy_static::lazy_static;
use tokio::sync::Mutex;

lazy_static! {
    pub static ref LOGSTORE: LogStore = LogStore::new();
}
pub static LOGGER: Logger = Logger;

pub struct Logger;

impl log::Log for Logger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= log::Level::Info
    }

    fn log(&self, record: &log::Record) {
        if self.enabled(record.metadata()) {
            let logs = Arc::clone(&LOGSTORE.logs);
            let time = chrono::Local::now();
            let msg = format!("[{}] {}", record.level(), record.args());
            tokio::spawn(async move {
                let mut logs = logs.lock().await;
                logs.push((time, msg));
            });
        }
    }

    fn flush(&self) {}
}

pub struct LogStore {
    pub logs: Arc<Mutex<Vec<(DateTime<Local>, String)>>>,
}

impl LogStore {
    fn new() -> LogStore {
        LogStore { logs: Arc::new(Mutex::new(vec![])) }
    }
}
