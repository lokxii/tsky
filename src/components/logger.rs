use std::sync::{Arc, Mutex};

use lazy_static::lazy_static;

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
            let msg = format!(
                "[{}][{}]{}",
                record.level(),
                chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
                record.args()
            );
            let mut logs = logs.lock().unwrap();
            logs.push(msg);
        }
    }

    fn flush(&self) {}
}

pub struct LogStore {
    pub logs: Arc<Mutex<Vec<String>>>,
}

impl LogStore {
    fn new() -> LogStore {
        LogStore { logs: Arc::new(Mutex::new(vec![])) }
    }
}
