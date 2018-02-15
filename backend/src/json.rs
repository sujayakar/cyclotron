use std::fs::File;
use std::io::{BufWriter, Write};
use serde_json;

use event::TraceEvent;
use state::Logger;

pub struct JsonWriter {
    file: BufWriter<File>,
}

impl JsonWriter {
    pub fn new(f: File) -> Self {
        JsonWriter { file: BufWriter::new(f) }
    }
}

impl Logger for JsonWriter {
    fn write(&mut self, event: TraceEvent) {
        serde_json::to_writer(&mut self.file, &event)
            .expect("Failed to write to logfile");
        self.file.write(b"\n").expect("Failed to write newline");
    }
    fn flush(&mut self) {
        self.file.flush().expect("Failed to flush");
    }
}
