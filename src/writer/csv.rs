use crate::error::Result;
use crate::flow::basic_flow::BasicFlow;
use crate::flow::feature::get_csv_header;
use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

pub struct CsvFlowWriter {
    file_path: PathBuf,
    writer: BufWriter<File>,
    flows_written: u64,
}

impl CsvFlowWriter {
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self> {
        let file_path = path.as_ref().to_path_buf();
        if let Some(parent) = file_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let is_new = !file_path.exists() || std::fs::metadata(&file_path)?.len() == 0;

        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&file_path)?;

        let mut writer = BufWriter::with_capacity(256 * 1024, file);

        if is_new {
            let header = get_csv_header();
            writeln!(writer, "{}", header)?;
            writer.flush()?;
        }

        Ok(Self {
            file_path,
            writer,
            flows_written: 0,
        })
    }

    pub fn write_flow(&mut self, flow: &BasicFlow) -> Result<()> {
        let features = flow.extract_features();
        let row = features.to_csv_row();
        writeln!(self.writer, "{}", row)?;
        self.flows_written += 1;
        Ok(())
    }

    pub fn write_flows<'a, I>(&mut self, flows: I) -> Result<u64>
    where
        I: IntoIterator<Item = &'a BasicFlow>,
    {
        let mut count = 0;
        for flow in flows {
            self.write_flow(flow)?;
            count += 1;
        }
        self.writer.flush()?;
        Ok(count)
    }

    pub fn flush(&mut self) -> Result<()> {
        self.writer.flush()?;
        Ok(())
    }

    pub fn flows_written(&self) -> u64 {
        self.flows_written
    }

    pub fn path(&self) -> &Path {
        &self.file_path
    }
}

impl Drop for CsvFlowWriter {
    fn drop(&mut self) {
        let _ = self.writer.flush();
    }
}
