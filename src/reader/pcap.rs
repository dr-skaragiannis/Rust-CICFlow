use crate::error::{CicError, Result};
use crate::packet::parser::PacketParser;
use crate::packet::info::BasicPacketInfo;
use byteorder::{BigEndian, ByteOrder, LittleEndian};
use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::Path;

pub enum Endianness {
    Big,
    Little,
}

pub struct PcapReader<R: Read> {
    reader: R,
    parser: PacketParser,
    endianness: Endianness,
    is_nanosecond: bool,
    link_type: u32,
    is_pcapng: bool,
    pcapng_interfaces: Vec<PcapngInterface>,
}

#[derive(Clone, Debug)]
struct PcapngInterface {
    link_type: u32,
    ts_resol: u8, // default 6 (10^-6 s)
}

impl PcapReader<BufReader<File>> {
    pub fn open<P: AsRef<Path>>(path: P, read_ipv4: bool, read_ipv6: bool) -> Result<Self> {
        let file = File::open(path)?;
        let reader = BufReader::with_capacity(512 * 1024, file);
        Self::new(reader, read_ipv4, read_ipv6)
    }
}

impl<R: Read + Seek> PcapReader<R> {
    pub fn new(mut reader: R, read_ipv4: bool, read_ipv6: bool) -> Result<Self> {
        let mut magic = [0u8; 4];
        reader.read_exact(&mut magic)?;
        reader.seek(SeekFrom::Start(0))?;

        let magic_u32_be = BigEndian::read_u32(&magic);

        if magic_u32_be == 0x0A0D0D0A {
            // PCAPNG
            let mut pcap_reader = Self {
                reader,
                parser: PacketParser::new(1, read_ipv4, read_ipv6),
                endianness: Endianness::Little,
                is_nanosecond: false,
                link_type: 1,
                is_pcapng: true,
                pcapng_interfaces: Vec::new(),
            };
            pcap_reader.init_pcapng()?;
            Ok(pcap_reader)
        } else {
            // Classic PCAP
            let (endianness, is_nanosecond) = match magic_u32_be {
                0xA1B2C3D4 => (Endianness::Big, false),
                0xD4C3B2A1 => (Endianness::Little, false),
                0xA1B23C4D => (Endianness::Big, true),
                0x4D3CB2A1 => (Endianness::Little, true),
                _ => {
                    return Err(CicError::Pcap(format!(
                        "Invalid PCAP magic number: 0x{:08X}",
                        magic_u32_be
                    )))
                }
            };

            let mut header_bytes = [0u8; 24];
            reader.read_exact(&mut header_bytes)?;

            let link_type = match endianness {
                Endianness::Big => BigEndian::read_u32(&header_bytes[20..24]),
                Endianness::Little => LittleEndian::read_u32(&header_bytes[20..24]),
            };

            let parser = PacketParser::new(link_type, read_ipv4, read_ipv6);

            Ok(Self {
                reader,
                parser,
                endianness,
                is_nanosecond,
                link_type,
                is_pcapng: false,
                pcapng_interfaces: Vec::new(),
            })
        }
    }

    fn init_pcapng(&mut self) -> Result<()> {
        // Read initial Section Header Block (SHB)
        let mut block_hdr = [0u8; 8];
        if self.reader.read_exact(&mut block_hdr).is_err() {
            return Ok(());
        }

        let block_type = LittleEndian::read_u32(&block_hdr[0..4]);
        let block_len = LittleEndian::read_u32(&block_hdr[4..8]) as usize;

        if block_type == 0x0A0D0D0A {
            let mut body = vec![0u8; block_len - 8];
            self.reader.read_exact(&mut body)?;
            if body.len() >= 4 {
                let bom = BigEndian::read_u32(&body[0..4]);
                if bom == 0x1A2B3C4D {
                    self.endianness = Endianness::Big;
                } else {
                    self.endianness = Endianness::Little;
                }
            }
        } else {
            self.reader.seek(SeekFrom::Start(0))?;
        }

        Ok(())
    }

    pub fn next_packet(&mut self) -> Result<Option<BasicPacketInfo>> {
        if self.is_pcapng {
            self.next_pcapng_packet()
        } else {
            self.next_classic_pcap_packet()
        }
    }

    fn next_classic_pcap_packet(&mut self) -> Result<Option<BasicPacketInfo>> {
        let mut pkt_hdr = [0u8; 16];
        match self.reader.read_exact(&mut pkt_hdr) {
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
            Err(e) => return Err(CicError::Io(e)),
        }

        let (ts_sec, ts_usec, incl_len, _orig_len) = match self.endianness {
            Endianness::Big => (
                BigEndian::read_u32(&pkt_hdr[0..4]),
                BigEndian::read_u32(&pkt_hdr[4..8]),
                BigEndian::read_u32(&pkt_hdr[8..12]),
                BigEndian::read_u32(&pkt_hdr[12..16]),
            ),
            Endianness::Little => (
                LittleEndian::read_u32(&pkt_hdr[0..4]),
                LittleEndian::read_u32(&pkt_hdr[4..8]),
                LittleEndian::read_u32(&pkt_hdr[8..12]),
                LittleEndian::read_u32(&pkt_hdr[12..16]),
            ),
        };

        let timestamp_us = if self.is_nanosecond {
            (ts_sec as i64) * 1_000_000 + (ts_usec as i64) / 1000
        } else {
            (ts_sec as i64) * 1_000_000 + (ts_usec as i64)
        };

        let mut pkt_data = vec![0u8; incl_len as usize];
        self.reader.read_exact(&mut pkt_data)?;

        self.parser.parse_packet(&pkt_data, timestamp_us)
    }

    fn next_pcapng_packet(&mut self) -> Result<Option<BasicPacketInfo>> {
        loop {
            let mut block_hdr = [0u8; 8];
            match self.reader.read_exact(&mut block_hdr) {
                Ok(_) => {}
                Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
                Err(e) => return Err(CicError::Io(e)),
            }

            let (block_type, block_len) = match self.endianness {
                Endianness::Big => (
                    BigEndian::read_u32(&block_hdr[0..4]),
                    BigEndian::read_u32(&block_hdr[4..8]) as usize,
                ),
                Endianness::Little => (
                    LittleEndian::read_u32(&block_hdr[0..4]),
                    LittleEndian::read_u32(&block_hdr[4..8]) as usize,
                ),
            };

            if block_len < 12 {
                return Err(CicError::Pcap(format!(
                    "Invalid PCAPNG block length: {}",
                    block_len
                )));
            }

            let mut body = vec![0u8; block_len - 8];
            self.reader.read_exact(&mut body)?;

            match block_type {
                0x00000001 => {
                    // Interface Description Block (IDB)
                    if body.len() >= 8 {
                        let link_type = match self.endianness {
                            Endianness::Big => BigEndian::read_u16(&body[0..2]) as u32,
                            Endianness::Little => LittleEndian::read_u16(&body[0..2]) as u32,
                        };
                        let mut ts_resol = 6u8;

                        // Parse options if present
                        let mut opt_offset = 8;
                        let body_data_len = body.len() - 4; // exclude trailing block length
                        while opt_offset + 4 <= body_data_len {
                            let (opt_code, opt_len) = match self.endianness {
                                Endianness::Big => (
                                    BigEndian::read_u16(&body[opt_offset..opt_offset + 2]),
                                    BigEndian::read_u16(&body[opt_offset + 2..opt_offset + 4])
                                        as usize,
                                ),
                                Endianness::Little => (
                                    LittleEndian::read_u16(&body[opt_offset..opt_offset + 2]),
                                    LittleEndian::read_u16(&body[opt_offset + 2..opt_offset + 4])
                                        as usize,
                                ),
                            };
                            if opt_code == 0 {
                                break;
                            }
                            if opt_code == 9 && opt_len == 1 && opt_offset + 4 < body_data_len {
                                ts_resol = body[opt_offset + 4];
                            }
                            let padded_len = (opt_len + 3) & !3;
                            opt_offset += 4 + padded_len;
                        }

                        self.pcapng_interfaces.push(PcapngInterface {
                            link_type,
                            ts_resol,
                        });
                    }
                }
                0x00000006 => {
                    // Enhanced Packet Block (EPB)
                    if body.len() >= 20 {
                        let (if_id, ts_high, ts_low, cap_len) = match self.endianness {
                            Endianness::Big => (
                                BigEndian::read_u32(&body[0..4]) as usize,
                                BigEndian::read_u32(&body[4..8]) as u64,
                                BigEndian::read_u32(&body[8..12]) as u64,
                                BigEndian::read_u32(&body[12..16]) as usize,
                            ),
                            Endianness::Little => (
                                LittleEndian::read_u32(&body[0..4]) as usize,
                                LittleEndian::read_u32(&body[4..8]) as u64,
                                LittleEndian::read_u32(&body[8..12]) as u64,
                                LittleEndian::read_u32(&body[12..16]) as usize,
                            ),
                        };

                        let raw_ts = (ts_high << 32) | ts_low;
                        let (link_type, ts_resol) = if if_id < self.pcapng_interfaces.len() {
                            (
                                self.pcapng_interfaces[if_id].link_type,
                                self.pcapng_interfaces[if_id].ts_resol,
                            )
                        } else {
                            (1, 6)
                        };

                        let timestamp_us = if ts_resol == 9 {
                            (raw_ts / 1000) as i64
                        } else if ts_resol == 6 {
                            raw_ts as i64
                        } else {
                            let factor = 10f64.powi(ts_resol as i32);
                            ((raw_ts as f64 / factor) * 1_000_000.0) as i64
                        };

                        if 20 + cap_len <= body.len() {
                            let pkt_data = &body[20..20 + cap_len];
                            self.parser.set_link_type(link_type);
                            if let Some(pkt_info) = self.parser.parse_packet(pkt_data, timestamp_us)? {
                                return Ok(Some(pkt_info));
                            }
                        }
                    }
                }
                0x00000003 => {
                    // Simple Packet Block (SPB)
                    if body.len() >= 4 {
                        let cap_len = body.len() - 8; // body length minus orig_len and trailing block len
                        let pkt_data = &body[4..4 + cap_len];
                        let link_type = self
                            .pcapng_interfaces
                            .first()
                            .map(|i| i.link_type)
                            .unwrap_or(1);
                        self.parser.set_link_type(link_type);
                        if let Some(pkt_info) = self.parser.parse_packet(pkt_data, 0)? {
                            return Ok(Some(pkt_info));
                        }
                    }
                }
                _ => {}
            }
        }
    }

    pub fn link_type(&self) -> u32 {
        self.link_type
    }
}
