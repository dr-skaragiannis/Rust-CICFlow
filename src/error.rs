use thiserror::Error;

#[derive(Error, Debug)]
pub enum CicError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("PCAP format error: {0}")]
    Pcap(String),

    #[error("Packet parse error: {0}")]
    PacketParse(String),

    #[error("CSV error: {0}")]
    Csv(#[from] csv::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[cfg(feature = "live-capture")]
    #[error("Live capture error: {0}")]
    LiveCapture(#[from] pcap::Error),

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Generic error: {0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, CicError>;
