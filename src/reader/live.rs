#[cfg(feature = "live-capture")]
use crate::error::{CicError, Result};
#[cfg(feature = "live-capture")]
use crate::packet::info::BasicPacketInfo;
#[cfg(feature = "live-capture")]
use crate::packet::parser::PacketParser;
#[cfg(feature = "live-capture")]
use pcap::{Capture, Device};

#[cfg(feature = "live-capture")]
pub struct LiveCapture {
    capture: Capture<pcap::Active>,
    parser: PacketParser,
}

#[cfg(feature = "live-capture")]
impl LiveCapture {
    pub fn new(
        device_name: &str,
        promiscuous: bool,
        snaplen: i32,
        timeout_ms: i32,
        read_ipv4: bool,
        read_ipv6: bool,
        bpf_filter: Option<&str>,
    ) -> Result<Self> {
        let mut cap = Capture::from_device(device_name)?
            .promisc(promiscuous)
            .snaplen(snaplen)
            .timeout(timeout_ms)
            .open()?;

        if let Some(filter) = bpf_filter {
            cap.filter(filter, true)?;
        }

        let link_type = cap.get_datalink().0 as u32;
        let parser = PacketParser::new(link_type, read_ipv4, read_ipv6);

        Ok(Self {
            capture: cap,
            parser,
        })
    }

    pub fn list_devices() -> Result<Vec<String>> {
        let devices = Device::list()?;
        Ok(devices.into_iter().map(|d| d.name).collect())
    }

    pub fn next_packet(&mut self) -> Result<Option<BasicPacketInfo>> {
        match self.capture.next_packet() {
            Ok(pkt) => {
                let ts = pkt.header.ts;
                let timestamp_us = (ts.tv_sec as i64) * 1_000_000 + (ts.tv_usec as i64);
                let pkt_info = self.parser.parse_packet(pkt.data, timestamp_us)?;
                Ok(pkt_info)
            }
            Err(pcap::Error::TimeoutExpired) => Ok(None),
            Err(e) => Err(CicError::LiveCapture(e)),
        }
    }
}
