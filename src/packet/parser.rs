use crate::error::Result;
use crate::packet::info::BasicPacketInfo;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

pub const LINKTYPE_NULL: u32 = 0;
pub const LINKTYPE_ETHERNET: u32 = 1;
pub const LINKTYPE_RAW_IPV4: u32 = 12;
pub const LINKTYPE_RAW_IPV6: u32 = 14;
pub const LINKTYPE_LOOPBACK: u32 = 108;
pub const LINKTYPE_LINUX_SLL: u32 = 113;
pub const LINKTYPE_LINUX_SLL2: u32 = 276;
pub const LINKTYPE_RAW_ALT: u32 = 101;
pub const LINKTYPE_RAW_IPV4_ALT: u32 = 228;
pub const LINKTYPE_RAW_IPV6_ALT: u32 = 229;

pub struct PacketParser {
    link_type: u32,
    read_ipv4: bool,
    read_ipv6: bool,
    packet_id_counter: u64,
}

impl PacketParser {
    pub fn new(link_type: u32, read_ipv4: bool, read_ipv6: bool) -> Self {
        Self {
            link_type,
            read_ipv4,
            read_ipv6,
            packet_id_counter: 0,
        }
    }

    pub fn set_link_type(&mut self, link_type: u32) {
        self.link_type = link_type;
    }

    pub fn parse_packet(&mut self, raw_data: &[u8], timestamp_us: i64) -> Result<Option<BasicPacketInfo>> {
        self.packet_id_counter += 1;
        let id = self.packet_id_counter;

        let network_data = match self.strip_link_layer(raw_data, self.link_type) {
            Ok(Some((ethertype, data))) => {
                if ethertype == 0x0800 {
                    if !self.read_ipv4 {
                        return Ok(None);
                    }
                    self.parse_ipv4(id, data, timestamp_us)?
                } else if ethertype == 0x86DD {
                    if !self.read_ipv6 {
                        return Ok(None);
                    }
                    self.parse_ipv6(id, data, timestamp_us)?
                } else {
                    // Try parsing as raw IPv4 or IPv6 by inspecting version nibble
                    if !data.is_empty() {
                        let version = data[0] >> 4;
                        if version == 4 && self.read_ipv4 {
                            self.parse_ipv4(id, data, timestamp_us)?
                        } else if version == 6 && self.read_ipv6 {
                            self.parse_ipv6(id, data, timestamp_us)?
                        } else {
                            return Ok(None);
                        }
                    } else {
                        return Ok(None);
                    }
                }
            }
            Ok(None) => return Ok(None),
            Err(_) => {
                // If link layer stripping fails, fallback to checking version nibble
                if !raw_data.is_empty() {
                    let version = raw_data[0] >> 4;
                    if version == 4 && self.read_ipv4 {
                        self.parse_ipv4(id, raw_data, timestamp_us)?
                    } else if version == 6 && self.read_ipv6 {
                        self.parse_ipv6(id, raw_data, timestamp_us)?
                    } else {
                        return Ok(None);
                    }
                } else {
                    return Ok(None);
                }
            }
        };

        Ok(network_data)
    }

    fn strip_link_layer<'a>(&self, data: &'a [u8], link_type: u32) -> Result<Option<(u16, &'a [u8])>> {
        match link_type {
            LINKTYPE_ETHERNET => {
                if data.len() < 14 {
                    return Ok(None);
                }
                let mut ethertype = u16::from_be_bytes([data[12], data[13]]);
                let mut offset = 14;

                // Handle 802.1Q (VLAN) and 802.1ad (QinQ) tags
                while (ethertype == 0x8100 || ethertype == 0x88A8 || ethertype == 0x9100)
                    && offset + 4 <= data.len()
                {
                    ethertype = u16::from_be_bytes([data[offset + 2], data[offset + 3]]);
                    offset += 4;
                }

                Ok(Some((ethertype, &data[offset..])))
            }
            LINKTYPE_LINUX_SLL => {
                // Linux cooked capture v1 (16 bytes header)
                if data.len() < 16 {
                    return Ok(None);
                }
                let ethertype = u16::from_be_bytes([data[14], data[15]]);
                Ok(Some((ethertype, &data[16..])))
            }
            LINKTYPE_LINUX_SLL2 => {
                // Linux cooked capture v2 (20 bytes header)
                if data.len() < 20 {
                    return Ok(None);
                }
                let ethertype = u16::from_be_bytes([data[0], data[1]]);
                Ok(Some((ethertype, &data[20..])))
            }
            LINKTYPE_NULL | LINKTYPE_LOOPBACK => {
                // 4-byte header containing address family
                if data.len() < 4 {
                    return Ok(None);
                }
                let family_be = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
                let family_le = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
                let ethertype = if family_be == 2 || family_le == 2 {
                    0x0800
                } else if family_be == 10
                    || family_le == 10
                    || family_be == 24
                    || family_le == 24
                    || family_be == 28
                    || family_le == 28
                    || family_be == 30
                    || family_le == 30
                {
                    0x86DD
                } else {
                    0
                };
                Ok(Some((ethertype, &data[4..])))
            }
            LINKTYPE_RAW_IPV4 | LINKTYPE_RAW_IPV4_ALT => Ok(Some((0x0800, data))),
            LINKTYPE_RAW_IPV6 | LINKTYPE_RAW_IPV6_ALT => Ok(Some((0x86DD, data))),
            LINKTYPE_RAW_ALT => {
                if !data.is_empty() {
                    let version = data[0] >> 4;
                    if version == 4 {
                        Ok(Some((0x0800, data)))
                    } else if version == 6 {
                        Ok(Some((0x86DD, data)))
                    } else {
                        Ok(None)
                    }
                } else {
                    Ok(None)
                }
            }
            _ => {
                // Default fallback
                if !data.is_empty() {
                    let version = data[0] >> 4;
                    if version == 4 {
                        Ok(Some((0x0800, data)))
                    } else if version == 6 {
                        Ok(Some((0x86DD, data)))
                    } else {
                        Ok(None)
                    }
                } else {
                    Ok(None)
                }
            }
        }
    }

    fn parse_ipv4(
        &self,
        id: u64,
        data: &[u8],
        timestamp_us: i64,
    ) -> Result<Option<BasicPacketInfo>> {
        if data.len() < 20 {
            return Ok(None);
        }

        let version_ihl = data[0];
        let version = version_ihl >> 4;
        if version != 4 {
            return Ok(None);
        }

        let ihl = (version_ihl & 0x0F) as usize * 4;
        if ihl < 20 || data.len() < ihl {
            return Ok(None);
        }

        let total_length = u16::from_be_bytes([data[2], data[3]]) as usize;
        let packet_length = total_length.min(data.len());
        if packet_length < ihl {
            return Ok(None);
        }

        let protocol = data[9];
        let src_bytes = data[12..16].to_vec();
        let dst_bytes = data[16..20].to_vec();
        let src_ip = IpAddr::V4(Ipv4Addr::new(
            src_bytes[0],
            src_bytes[1],
            src_bytes[2],
            src_bytes[3],
        ));
        let dst_ip = IpAddr::V4(Ipv4Addr::new(
            dst_bytes[0],
            dst_bytes[1],
            dst_bytes[2],
            dst_bytes[3],
        ));

        let l4_data = &data[ihl..packet_length];
        self.parse_transport(
            id,
            src_ip,
            dst_ip,
            src_bytes,
            dst_bytes,
            protocol,
            timestamp_us,
            l4_data,
        )
    }

    fn parse_ipv6(
        &self,
        id: u64,
        data: &[u8],
        timestamp_us: i64,
    ) -> Result<Option<BasicPacketInfo>> {
        if data.len() < 40 {
            return Ok(None);
        }

        let version = data[0] >> 4;
        if version != 6 {
            return Ok(None);
        }

        let payload_length = u16::from_be_bytes([data[4], data[5]]) as usize;
        let mut next_header = data[6];
        let src_bytes = data[8..24].to_vec();
        let dst_bytes = data[24..40].to_vec();

        let mut src_arr = [0u8; 16];
        src_arr.copy_from_slice(&src_bytes);
        let mut dst_arr = [0u8; 16];
        dst_arr.copy_from_slice(&dst_bytes);

        let src_ip = IpAddr::V6(Ipv6Addr::from(src_arr));
        let dst_ip = IpAddr::V6(Ipv6Addr::from(dst_arr));

        let mut offset = 40;
        let max_len = (40 + payload_length).min(data.len());

        // Traverse extension headers
        while offset < max_len {
            match next_header {
                0 | 43 | 60 => {
                    // Hop-by-Hop (0), Routing (43), Destination Options (60)
                    if offset + 2 > max_len {
                        break;
                    }
                    let next = data[offset];
                    let ext_len = ((data[offset + 1] as usize) + 1) * 8;
                    if offset + ext_len > max_len {
                        break;
                    }
                    next_header = next;
                    offset += ext_len;
                }
                44 => {
                    // Fragment header (8 bytes fixed)
                    if offset + 8 > max_len {
                        break;
                    }
                    next_header = data[offset];
                    offset += 8;
                }
                51 => {
                    // AH (Authentication Header)
                    if offset + 2 > max_len {
                        break;
                    }
                    let next = data[offset];
                    let ext_len = ((data[offset + 1] as usize) + 2) * 4;
                    if offset + ext_len > max_len {
                        break;
                    }
                    next_header = next;
                    offset += ext_len;
                }
                _ => {
                    break;
                }
            }
        }

        if offset > max_len {
            return Ok(None);
        }

        let l4_data = &data[offset..max_len];
        self.parse_transport(
            id,
            src_ip,
            dst_ip,
            src_bytes,
            dst_bytes,
            next_header,
            timestamp_us,
            l4_data,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn parse_transport(
        &self,
        id: u64,
        src_ip: IpAddr,
        dst_ip: IpAddr,
        src_bytes: Vec<u8>,
        dst_bytes: Vec<u8>,
        protocol: u8,
        timestamp_us: i64,
        l4_data: &[u8],
    ) -> Result<Option<BasicPacketInfo>> {
        match protocol {
            6 => {
                // TCP
                if l4_data.len() < 20 {
                    return Ok(None);
                }
                let src_port = u16::from_be_bytes([l4_data[0], l4_data[1]]);
                let dst_port = u16::from_be_bytes([l4_data[2], l4_data[3]]);
                let data_offset = ((l4_data[12] >> 4) as usize) * 4;
                if data_offset < 20 || l4_data.len() < data_offset {
                    return Ok(None);
                }

                let flags_byte = l4_data[13];
                let flag_cwr = (flags_byte & 0x80) != 0;
                let flag_ece = (flags_byte & 0x40) != 0;
                let flag_urg = (flags_byte & 0x20) != 0;
                let flag_ack = (flags_byte & 0x10) != 0;
                let flag_psh = (flags_byte & 0x08) != 0;
                let flag_rst = (flags_byte & 0x04) != 0;
                let flag_syn = (flags_byte & 0x02) != 0;
                let flag_fin = (flags_byte & 0x01) != 0;

                let tcp_window = u16::from_be_bytes([l4_data[14], l4_data[15]]) as u32;
                let header_bytes = data_offset as u64;
                let payload_bytes = (l4_data.len() - data_offset) as u64;

                let pkt = BasicPacketInfo::new(
                    id,
                    src_ip,
                    dst_ip,
                    src_bytes,
                    dst_bytes,
                    src_port,
                    dst_port,
                    6,
                    timestamp_us,
                    payload_bytes,
                    header_bytes,
                    tcp_window,
                    (
                        flag_fin, flag_syn, flag_rst, flag_psh, flag_ack, flag_urg, flag_ece,
                        flag_cwr,
                    ),
                );
                Ok(Some(pkt))
            }
            17 => {
                // UDP
                if l4_data.len() < 8 {
                    return Ok(None);
                }
                let src_port = u16::from_be_bytes([l4_data[0], l4_data[1]]);
                let dst_port = u16::from_be_bytes([l4_data[2], l4_data[3]]);
                let udp_length = u16::from_be_bytes([l4_data[4], l4_data[5]]) as usize;
                let header_bytes = 8u64;
                let payload_bytes = if udp_length >= 8 {
                    (udp_length - 8).min(l4_data.len() - 8) as u64
                } else {
                    0u64
                };

                let pkt = BasicPacketInfo::new(
                    id,
                    src_ip,
                    dst_ip,
                    src_bytes,
                    dst_bytes,
                    src_port,
                    dst_port,
                    17,
                    timestamp_us,
                    payload_bytes,
                    header_bytes,
                    0,
                    (false, false, false, false, false, false, false, false),
                );
                Ok(Some(pkt))
            }
            _ => {
                // Other protocol (ICMP, SCTP, IGMP, etc.)
                let (src_port, dst_port, header_bytes, payload_bytes) = if l4_data.len() >= 4 {
                    let sp = u16::from_be_bytes([l4_data[0], l4_data[1]]);
                    let dp = u16::from_be_bytes([l4_data[2], l4_data[3]]);
                    (sp, dp, 0u64, l4_data.len() as u64)
                } else {
                    (0, 0, 0u64, l4_data.len() as u64)
                };

                let pkt = BasicPacketInfo::new(
                    id,
                    src_ip,
                    dst_ip,
                    src_bytes,
                    dst_bytes,
                    src_port,
                    dst_port,
                    protocol,
                    timestamp_us,
                    payload_bytes,
                    header_bytes,
                    0,
                    (false, false, false, false, false, false, false, false),
                );
                Ok(Some(pkt))
            }
        }
    }
}
