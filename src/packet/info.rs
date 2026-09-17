use std::net::IpAddr;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BasicPacketInfo {
    pub id: u64,
    pub src_ip: IpAddr,
    pub dst_ip: IpAddr,
    pub src_bytes: Vec<u8>,
    pub dst_bytes: Vec<u8>,
    pub src_port: u16,
    pub dst_port: u16,
    pub protocol: u8,
    pub timestamp: i64, // microseconds
    pub payload_bytes: u64,
    pub header_bytes: u64,
    pub tcp_window: u32,
    pub flag_fin: bool,
    pub flag_syn: bool,
    pub flag_rst: bool,
    pub flag_psh: bool,
    pub flag_ack: bool,
    pub flag_urg: bool,
    pub flag_ece: bool,
    pub flag_cwr: bool,
    pub flow_id: Option<String>,
}

impl BasicPacketInfo {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: u64,
        src_ip: IpAddr,
        dst_ip: IpAddr,
        src_bytes: Vec<u8>,
        dst_bytes: Vec<u8>,
        src_port: u16,
        dst_port: u16,
        protocol: u8,
        timestamp: i64,
        payload_bytes: u64,
        header_bytes: u64,
        tcp_window: u32,
        flags: (bool, bool, bool, bool, bool, bool, bool, bool), // (FIN, SYN, RST, PSH, ACK, URG, ECE, CWR)
    ) -> Self {
        let (flag_fin, flag_syn, flag_rst, flag_psh, flag_ack, flag_urg, flag_ece, flag_cwr) = flags;
        let mut pkt = Self {
            id,
            src_ip,
            dst_ip,
            src_bytes,
            dst_bytes,
            src_port,
            dst_port,
            protocol,
            timestamp,
            payload_bytes,
            header_bytes,
            tcp_window,
            flag_fin,
            flag_syn,
            flag_rst,
            flag_psh,
            flag_ack,
            flag_urg,
            flag_ece,
            flag_cwr,
            flow_id: None,
        };
        pkt.flow_id = Some(pkt.generate_flow_id(false));
        pkt
    }

    /// Generates bidirectional canonical Flow ID.
    /// If `compat_mode` is true, replicates Java's signed byte comparison.
    pub fn generate_flow_id(&self, compat_mode: bool) -> String {
        let mut forward = true;
        let len = self.src_bytes.len().min(self.dst_bytes.len());

        for i in 0..len {
            if compat_mode {
                let s = self.src_bytes[i] as i8;
                let d = self.dst_bytes[i] as i8;
                if s != d {
                    if s > d {
                        forward = false;
                    }
                    break;
                }
            } else {
                let s = self.src_bytes[i];
                let d = self.dst_bytes[i];
                if s != d {
                    if s > d {
                        forward = false;
                    }
                    break;
                }
            }
        }

        if forward {
            format!(
                "{}-{}-{}-{}-{}",
                self.src_ip, self.dst_ip, self.src_port, self.dst_port, self.protocol
            )
        } else {
            format!(
                "{}-{}-{}-{}-{}",
                self.dst_ip, self.src_ip, self.dst_port, self.src_port, self.protocol
            )
        }
    }

    #[inline]
    pub fn fwd_flow_id(&self) -> String {
        format!(
            "{}-{}-{}-{}-{}",
            self.src_ip, self.dst_ip, self.src_port, self.dst_port, self.protocol
        )
    }

    #[inline]
    pub fn bwd_flow_id(&self) -> String {
        format!(
            "{}-{}-{}-{}-{}",
            self.dst_ip, self.src_ip, self.dst_port, self.src_port, self.protocol
        )
    }

    #[inline]
    pub fn is_forward_packet(&self, first_src_bytes: &[u8]) -> bool {
        self.src_bytes.as_slice() == first_src_bytes
    }
}
