use std::fs::File;
use std::io::{BufWriter, Write};
use std::sync::{Arc, Mutex};
use serde_json;

use event::TraceEvent;
use state::Logger;

#[derive(Clone)]
pub struct JsonWriter {
    file: Arc<Mutex<BufWriter<File>>>,
}

impl JsonWriter {
    pub fn new(f: File) -> Self {
        JsonWriter { file: Arc::new(Mutex::new(BufWriter::new(f))) }
    }

    pub fn flush(&self) {
        self.file.lock().unwrap().flush().expect("Failed to flush");
    }
}

impl Logger for JsonWriter {
    fn write(&mut self, event: TraceEvent) {
        let mut file = self.file.lock().unwrap();
        serde_json::to_writer(&mut *file, &event)
            .expect("Failed to write to logfile");
        file.write(b"\n").expect("Failed to write newline");
    }
}
