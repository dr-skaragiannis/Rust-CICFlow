//! # CICFlowMeter Rust
//!
//! A high-performance, memory-safe network traffic flow feature generator and PCAP analyzer
//! implemented in pure Rust.
//!
//! ## Overview
//! CICFlowMeter extracts 84 bidirectional statistical network flow features from live network
//! interfaces or offline PCAP/PCAPNG capture files. These features are widely used in machine
//! learning datasets such as CIC-IDS2017, CIC-IDS2018, ISCX, and modern network intrusion detection.
//!
//! ## Key Highlights
//! - **High Performance**: Zero-copy packet parsing and streaming statistical updates.
//! - **Broad Protocol Support**: Ethernet, Linux Cooked (SLL/SLL2), Loopback, VLAN (802.1Q/802.1ad), IPv4, IPv6, TCP, UDP.
//! - **Full CIC Compatibility**: Generates all 84 bidirectional features matching the canonical format.
//! - **Multi-Core Parallelism**: Rayon-powered parallel processing for batch PCAP directory conversions.
//! - **Live and Offline**: Full support for both live interface listening and offline capture files (PCAP and PCAPNG).
//! - **Flexible Outputs**: Standard CSV, JSON, and JSONL format support.

pub mod error;
pub mod packet;
pub mod flow;
pub mod reader;
pub mod writer;
pub mod engine;

pub use error::{CicError, Result};
pub use packet::{BasicPacketInfo, PacketParser};
pub use flow::{
    BasicFlow, BulkTracker, FlowFeatureType, FlowFeatures, FlowGenerator, FlowKey,
    SummaryStats, SubflowTracker, get_csv_header, CIC_FEATURE_NAMES,
};
pub use reader::PcapReader;
#[cfg(feature = "live-capture")]
pub use reader::LiveCapture;
pub use writer::{CsvFlowWriter, JsonFlowWriter};
pub use engine::{EngineConfig, FlowEngine, OutputFormat, ProcessingStats, is_pcap_file};

pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
