use crate::error::{CicError, Result};
use crate::flow::generator::FlowGenerator;
use crate::reader::PcapReader;
use crate::writer::{CsvFlowWriter, JsonFlowWriter};
#[cfg(feature = "live-capture")]
use crate::reader::LiveCapture;
use indicatif::{ProgressBar, ProgressStyle};
use rayon::prelude::*;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutputFormat {
    Csv,
    Json,
    Jsonl,
}

impl Default for OutputFormat {
    fn default() -> Self {
        Self::Csv
    }
}

impl std::str::FromStr for OutputFormat {
    type Err = CicError;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "csv" => Ok(OutputFormat::Csv),
            "json" => Ok(OutputFormat::Json),
            "jsonl" | "ndjson" => Ok(OutputFormat::Jsonl),
            _ => Err(CicError::Config(format!("Unsupported output format: {}", s))),
        }
    }
}

#[derive(Clone, Debug)]
pub struct EngineConfig {
    pub bidirectional: bool,
    pub flow_timeout_us: i64,
    pub activity_timeout_us: i64,
    pub read_ipv4: bool,
    pub read_ipv6: bool,
    pub label: Option<String>,
    pub compat_mode: bool,
    pub min_packets_per_flow: u64,
    pub format: OutputFormat,
    pub threads: usize,
    pub show_progress: bool,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            bidirectional: true,
            flow_timeout_us: 120_000_000,
            activity_timeout_us: 5_000_000,
            read_ipv4: true,
            read_ipv6: true,
            label: Some("NeedManualLabel".to_string()),
            compat_mode: false,
            min_packets_per_flow: 2,
            format: OutputFormat::Csv,
            threads: rayon::current_num_threads(),
            show_progress: true,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct ProcessingStats {
    pub total_packets: u64,
    pub valid_packets: u64,
    pub discarded_packets: u64,
    pub total_flows: u64,
    pub elapsed_ms: u128,
}

pub struct FlowEngine {
    config: EngineConfig,
}

impl FlowEngine {
    pub fn new(config: EngineConfig) -> Self {
        Self { config }
    }

    /// Process a single PCAP / PCAPNG file and write to out_path
    pub fn process_file<P: AsRef<Path>, Q: AsRef<Path>>(
        &self,
        input_pcap: P,
        output_file: Q,
    ) -> Result<ProcessingStats> {
        let start_time = Instant::now();
        let mut reader = PcapReader::open(input_pcap.as_ref(), self.config.read_ipv4, self.config.read_ipv6)?;

        let mut generator = FlowGenerator::new(
            self.config.bidirectional,
            self.config.flow_timeout_us,
            self.config.activity_timeout_us,
            self.config.label.clone(),
            self.config.compat_mode,
        );
        generator.set_min_packets_per_flow(self.config.min_packets_per_flow);

        let mut total_packets = 0u64;
        let mut valid_packets = 0u64;
        let discarded_packets = 0u64;

        // Writer setup
        let out_path = output_file.as_ref();
        let mut csv_writer = if self.config.format == OutputFormat::Csv {
            Some(CsvFlowWriter::new(out_path)?)
        } else {
            None
        };

        let mut json_writer = match self.config.format {
            OutputFormat::Json => Some(JsonFlowWriter::new(out_path, false)?),
            OutputFormat::Jsonl => Some(JsonFlowWriter::new(out_path, true)?),
            _ => None,
        };

        while let Some(packet_info) = reader.next_packet()? {
            total_packets += 1;
            valid_packets += 1;
            generator.add_packet(packet_info);

            // Flush finished flows periodically to disk to keep memory consumption near zero
            let finished = generator.get_finished_flows();
            for flow in &finished {
                if let Some(ref mut w) = csv_writer {
                    w.write_flow(flow)?;
                } else if let Some(ref mut w) = json_writer {
                    w.write_flow(flow)?;
                }
            }
        }

        // Flush remaining flows at end of stream
        let remaining = generator.finish_all_flows();
        for flow in &remaining {
            if let Some(ref mut w) = csv_writer {
                w.write_flow(flow)?;
            } else if let Some(ref mut w) = json_writer {
                w.write_flow(flow)?;
            }
        }

        if let Some(ref mut w) = csv_writer {
            w.flush()?;
        }
        if let Some(ref mut w) = json_writer {
            w.close()?;
        }

        let elapsed = start_time.elapsed().as_millis();
        let total_flows = generator.total_finished_flows();

        Ok(ProcessingStats {
            total_packets,
            valid_packets,
            discarded_packets,
            total_flows,
            elapsed_ms: elapsed,
        })
    }

    /// Process an input path (either a single file or a directory containing PCAP files)
    pub fn process_path<P: AsRef<Path>, Q: AsRef<Path>>(
        &self,
        input_path: P,
        output_dir: Q,
    ) -> Result<Vec<(PathBuf, ProcessingStats)>> {
        let in_path = input_path.as_ref();
        let out_dir = output_dir.as_ref();

        std::fs::create_dir_all(out_dir)?;

        let mut pcap_files = Vec::new();
        if in_path.is_dir() {
            for entry in std::fs::read_dir(in_path)? {
                let entry = entry?;
                let path = entry.path();
                if path.is_file() && is_pcap_file(&path) {
                    pcap_files.push(path);
                }
            }
            pcap_files.sort();
        } else if in_path.is_file() {
            pcap_files.push(in_path.to_path_buf());
        } else {
            return Err(CicError::Config(format!(
                "Input path does not exist: {}",
                in_path.display()
            )));
        }

        if pcap_files.is_empty() {
            return Err(CicError::Config(format!(
                "No PCAP files found in {}",
                in_path.display()
            )));
        }

        let ext = match self.config.format {
            OutputFormat::Csv => "_Flow.csv",
            OutputFormat::Json => "_Flow.json",
            OutputFormat::Jsonl => "_Flow.jsonl",
        };

        let pb = if self.config.show_progress && pcap_files.len() > 1 {
            let p = ProgressBar::new(pcap_files.len() as u64);
            p.set_style(
                ProgressStyle::default_bar()
                    .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({eta}) {msg}")
                    .unwrap()
                    .progress_chars("#>-"),
            );
            Some(p)
        } else {
            None
        };

        // Determine multi-threading strategy
        let results: Vec<(PathBuf, ProcessingStats)> = if pcap_files.len() > 1 && self.config.threads > 1 {
            let pool = rayon::ThreadPoolBuilder::new()
                .num_threads(self.config.threads)
                .build()
                .unwrap();

            pool.install(|| {
                pcap_files
                    .par_iter()
                    .map(|pcap_file| {
                        let file_stem = pcap_file
                            .file_name()
                            .and_then(|s| s.to_str())
                            .unwrap_or("output");
                        let out_file_name = format!("{}{}", file_stem, ext);
                        let out_file = out_dir.join(out_file_name);

                        let stats = self.process_file(pcap_file, &out_file).unwrap_or_default();
                        if let Some(ref p) = pb {
                            p.inc(1);
                        }
                        (pcap_file.clone(), stats)
                    })
                    .collect()
            })
        } else {
            pcap_files
                .iter()
                .map(|pcap_file| {
                    let file_stem = pcap_file
                        .file_name()
                        .and_then(|s| s.to_str())
                        .unwrap_or("output");
                    let out_file_name = format!("{}{}", file_stem, ext);
                    let out_file = out_dir.join(out_file_name);

                    if let Some(ref p) = pb {
                        p.set_message(format!("Processing {}", file_stem));
                    }

                    let stats = self.process_file(pcap_file, &out_file)?;
                    if let Some(ref p) = pb {
                        p.inc(1);
                    }
                    Ok((pcap_file.clone(), stats))
                })
                .collect::<Result<Vec<_>>>()?
        };

        if let Some(p) = pb {
            p.finish_with_message("Completed all PCAP processing.");
        }

        Ok(results)
    }

    /// Process live interface traffic
    #[cfg(feature = "live-capture")]
    pub fn process_live<Q: AsRef<Path>>(
        &self,
        device_name: &str,
        output_file: Q,
        bpf_filter: Option<&str>,
        running_flag: Arc<AtomicBool>,
    ) -> Result<ProcessingStats> {
        let start_time = Instant::now();
        let mut capture = LiveCapture::new(
            device_name,
            true,
            65535,
            1000,
            self.config.read_ipv4,
            self.config.read_ipv6,
            bpf_filter,
        )?;

        let mut generator = FlowGenerator::new(
            self.config.bidirectional,
            self.config.flow_timeout_us,
            self.config.activity_timeout_us,
            self.config.label.clone(),
            self.config.compat_mode,
        );
        generator.set_min_packets_per_flow(self.config.min_packets_per_flow);

        let out_path = output_file.as_ref();
        let mut csv_writer = if self.config.format == OutputFormat::Csv {
            Some(CsvFlowWriter::new(out_path)?)
        } else {
            None
        };

        let mut json_writer = match self.config.format {
            OutputFormat::Json => Some(JsonFlowWriter::new(out_path, false)?),
            OutputFormat::Jsonl => Some(JsonFlowWriter::new(out_path, true)?),
            _ => None,
        };

        let mut total_packets = 0u64;
        let mut valid_packets = 0u64;

        log::info!("Starting live capture on device: {}", device_name);

        while running_flag.load(Ordering::Relaxed) {
            if let Some(pkt) = capture.next_packet()? {
                total_packets += 1;
                valid_packets += 1;
                generator.add_packet(pkt);

                let finished = generator.get_finished_flows();
                for flow in &finished {
                    if let Some(ref mut w) = csv_writer {
                        w.write_flow(flow)?;
                        w.flush()?;
                    } else if let Some(ref mut w) = json_writer {
                        w.write_flow(flow)?;
                        w.flush()?;
                    }
                }
            }
        }

        let remaining = generator.finish_all_flows();
        for flow in &remaining {
            if let Some(ref mut w) = csv_writer {
                w.write_flow(flow)?;
            } else if let Some(ref mut w) = json_writer {
                w.write_flow(flow)?;
            }
        }

        if let Some(ref mut w) = csv_writer {
            w.flush()?;
        }
        if let Some(ref mut w) = json_writer {
            w.close()?;
        }

        let elapsed = start_time.elapsed().as_millis();
        let total_flows = generator.total_finished_flows();

        Ok(ProcessingStats {
            total_packets,
            valid_packets,
            discarded_packets: 0,
            total_flows,
            elapsed_ms: elapsed,
        })
    }
}

pub fn is_pcap_file(path: &Path) -> bool {
    if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
        let ext_lower = ext.to_lowercase();
        if ext_lower == "pcap"
            || ext_lower == "pcapng"
            || ext_lower == "cap"
            || ext_lower == "dmp"
        {
            return true;
        }
    }

    // Check magic bytes if extension not obvious
    if let Ok(mut f) = std::fs::File::open(path) {
        use std::io::Read;
        let mut magic = [0u8; 4];
        if f.read_exact(&mut magic).is_ok() {
            let m = u32::from_be_bytes(magic);
            return m == 0xA1B2C3D4
                || m == 0xD4C3B2A1
                || m == 0xA1B23C4D
                || m == 0x4D3CB2A1
                || m == 0x0A0D0D0A;
        }
    }

    false
}
