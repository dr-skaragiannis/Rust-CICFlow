//! Regression test: non-IP frames (ARP, LLDP, ...) in a classic PCAP must be skipped,
//! not treated as end-of-file. Before the fix, `next_packet()` returned `Ok(None)` for the
//! first ARP frame and the caller's `while let Some(..)` loop stopped reading, silently
//! truncating every capture that contained one.

use cicflowmeter::PcapReader;
use std::fs::File;
use std::io::Write;
use tempfile::tempdir;

fn tcp_frame(sport: u16, dport: u16, flags: u8) -> Vec<u8> {
    let mut raw = Vec::new();
    raw.extend_from_slice(&[0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb, 0x08, 0x00]);
    raw.extend_from_slice(&[0x45, 0x00, 0x00, 40, 0x12, 0x34, 0x40, 0x00, 64, 6, 0x00, 0x00]);
    raw.extend_from_slice(&[10, 0, 0, 5, 203, 0, 113, 7]);
    raw.extend_from_slice(&sport.to_be_bytes());
    raw.extend_from_slice(&dport.to_be_bytes());
    raw.extend_from_slice(&[0, 0, 0, 1, 0, 0, 0, 1, 0x50, flags, 0x7f, 0xff, 0, 0, 0, 0]);
    raw
}

/// A 60-byte ARP request (42 bytes + Ethernet padding), as captured on real networks.
fn arp_frame() -> Vec<u8> {
    let mut raw = Vec::new();
    raw.extend_from_slice(&[0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x08, 0x06]);
    raw.extend_from_slice(&[0x00, 0x01, 0x08, 0x00, 0x06, 0x04, 0x00, 0x01]);
    raw.extend_from_slice(&[0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 10, 0, 0, 5]);
    raw.extend_from_slice(&[0, 0, 0, 0, 0, 0, 10, 0, 0, 1]);
    raw.resize(60, 0);
    raw
}

fn write_pcap(path: &std::path::Path, frames: &[Vec<u8>]) {
    let mut f = File::create(path).unwrap();
    f.write_all(&[0xd4, 0xc3, 0xb2, 0xa1, 2, 0, 4, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0xff, 0xff, 0, 0, 1, 0, 0, 0]).unwrap();
    for (i, frame) in frames.iter().enumerate() {
        let mut hdr = [0u8; 16];
        hdr[0..4].copy_from_slice(&1u32.to_le_bytes());
        hdr[4..8].copy_from_slice(&((i as u32) * 1000).to_le_bytes());
        hdr[8..12].copy_from_slice(&(frame.len() as u32).to_le_bytes());
        hdr[12..16].copy_from_slice(&(frame.len() as u32).to_le_bytes());
        f.write_all(&hdr).unwrap();
        f.write_all(frame).unwrap();
    }
}

fn count_packets(path: &std::path::Path) -> usize {
    let mut reader = PcapReader::open(path, true, true).unwrap();
    let mut n = 0;
    while let Some(_pkt) = reader.next_packet().unwrap() {
        n += 1;
    }
    n
}

#[test]
fn classic_pcap_skips_arp_instead_of_stopping() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("with_arp.pcap");
    write_pcap(&path, &[
        tcp_frame(50000, 443, 0x02),
        arp_frame(),
        tcp_frame(50000, 443, 0x10),
        arp_frame(),
        arp_frame(),
        tcp_frame(50001, 8133, 0x02),
    ]);
    // 3 IP packets; the 3 ARP frames between them are skipped without ending the read.
    assert_eq!(count_packets(&path), 3);
}

#[test]
fn classic_pcap_ending_with_non_ip_frames_terminates() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("arp_tail.pcap");
    write_pcap(&path, &[tcp_frame(50000, 443, 0x02), arp_frame(), arp_frame()]);
    assert_eq!(count_packets(&path), 1);
}

#[test]
fn classic_pcap_with_only_non_ip_frames_yields_nothing() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("arp_only.pcap");
    write_pcap(&path, &[arp_frame(), arp_frame()]);
    assert_eq!(count_packets(&path), 0);
}
