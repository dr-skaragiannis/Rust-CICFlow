use crate::error::Result;
use crate::flow::basic_flow::BasicFlow;
use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

pub struct JsonFlowWriter {
    pub file_path: PathBuf,
    writer: BufWriter<File>,
    flows_written: u64,
    is_array: bool,
    first_item: bool,
    closed: bool,
}

impl JsonFlowWriter {
    pub fn new<P: AsRef<Path>>(path: P, is_jsonl: bool) -> Result<Self> {
        let file_path = path.as_ref().to_path_buf();
        if let Some(parent) = file_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&file_path)?;

        let mut writer = BufWriter::with_capacity(256 * 1024, file);

        if !is_jsonl {
            writeln!(writer, "[")?;
        }

        Ok(Self {
            file_path,
            writer,
            flows_written: 0,
            is_array: !is_jsonl,
            first_item: true,
            closed: false,
        })
    }

    pub fn write_flow(&mut self, flow: &BasicFlow) -> Result<()> {
        let features = flow.extract_features();
        let json_str = serde_json::to_string(&features)?;

        if self.is_array {
            if !self.first_item {
                writeln!(self.writer, ",")?;
            }
            write!(self.writer, "  {}", json_str)?;
            self.first_item = false;
        } else {
            writeln!(self.writer, "{}", json_str)?;
        }

        self.flows_written += 1;
        Ok(())
    }

    pub fn flush(&mut self) -> Result<()> {
        self.writer.flush()?;
        Ok(())
    }

    pub fn close(&mut self) -> Result<()> {
        if !self.closed {
            if self.is_array {
                writeln!(self.writer, "\n]")?;
            }
            self.writer.flush()?;
            self.closed = true;
        }
        Ok(())
    }

    pub fn flows_written(&self) -> u64 {
        self.flows_written
    }
}

impl Drop for JsonFlowWriter {
    fn drop(&mut self) {
        let _ = self.close();
    }
}
