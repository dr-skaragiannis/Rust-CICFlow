use cicflowmeter::packet::parser::{PacketParser, LINKTYPE_ETHERNET, LINKTYPE_LINUX_SLL};
use std::net::IpAddr;

#[test]
fn test_parse_ethernet_ipv4_tcp() {
    let mut parser = PacketParser::new(LINKTYPE_ETHERNET, true, true);

    // Construct Ethernet + IPv4 + TCP packet
    let mut raw = Vec::new();
    // Ethernet: Dst MAC (6), Src MAC (6), EtherType 0x0800 (2)
    raw.extend_from_slice(&[0x00, 0x11, 0x22, 0x33, 0x44, 0x55]);
    raw.extend_from_slice(&[0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb]);
    raw.extend_from_slice(&[0x08, 0x00]);

    // IPv4: Version 4, IHL 5 (20 bytes), Total len 44 (20 IP + 24 TCP), Proto TCP (6), Src 192.168.1.100, Dst 10.0.0.1
    let ip_header = [
        0x45, 0x00, 0x00, 0x2c, // Ver/IHL, DSCP, Total Length = 44
        0x12, 0x34, 0x40, 0x00, // ID, Flags/Frag
        0x40, 0x06, 0x00, 0x00, // TTL = 64, Proto = TCP (6), Checksum
        192, 168, 1, 100,       // Src IP
        10, 0, 0, 1,            // Dst IP
    ];
    raw.extend_from_slice(&ip_header);

    // TCP: Src Port 54321, Dst Port 80, Seq 1, Ack 0, Data Offset 6 (24 bytes), SYN flag (0x02), Window 64240
    let tcp_header = [
        0xd4, 0x31, 0x00, 0x50, // Src Port 54321, Dst Port 80
        0x00, 0x00, 0x00, 0x01, // Seq
        0x00, 0x00, 0x00, 0x00, // Ack
        0x60, 0x02, 0xfa, 0xf0, // Data Offset (6 = 24 bytes), SYN flag (0x02), Win 64240
        0x00, 0x00, 0x00, 0x00, // Checksum, Urgent
        0x02, 0x04, 0x05, 0xb4, // Options: MSS 1460 (4 bytes)
    ];
    raw.extend_from_slice(&tcp_header);

    let pkt = parser.parse_packet(&raw, 1000000).unwrap().expect("Failed to parse packet");

    assert_eq!(pkt.src_ip, "192.168.1.100".parse::<IpAddr>().unwrap());
    assert_eq!(pkt.dst_ip, "10.0.0.1".parse::<IpAddr>().unwrap());
    assert_eq!(pkt.src_port, 54321);
    assert_eq!(pkt.dst_port, 80);
    assert_eq!(pkt.protocol, 6);
    assert!(pkt.flag_syn);
    assert!(!pkt.flag_ack);
    assert_eq!(pkt.tcp_window, 64240);
    assert_eq!(pkt.header_bytes, 24);
    assert_eq!(pkt.payload_bytes, 0);
    assert_eq!(pkt.timestamp, 1000000);
}

#[test]
fn test_parse_vlan_tagged_packet() {
    let mut parser = PacketParser::new(LINKTYPE_ETHERNET, true, true);

    let mut raw = Vec::new();
    // Ethernet: Dst MAC (6), Src MAC (6), EtherType 0x8100 (802.1Q VLAN)
    raw.extend_from_slice(&[0x00, 0x11, 0x22, 0x33, 0x44, 0x55]);
    raw.extend_from_slice(&[0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb]);
    raw.extend_from_slice(&[0x81, 0x00]); // 802.1Q
    raw.extend_from_slice(&[0x00, 0x64]); // VLAN ID 100
    raw.extend_from_slice(&[0x08, 0x00]); // Inner EtherType IPv4

    // IPv4 UDP
    let ip_header = [
        0x45, 0x00, 0x00, 0x20, // Total Length = 32
        0x00, 0x01, 0x00, 0x00,
        0x40, 0x11, 0x00, 0x00, // Proto = UDP (17)
        10, 1, 2, 3,
        10, 4, 5, 6,
    ];
    raw.extend_from_slice(&ip_header);

    // UDP: Src 12345, Dst 53, Len 12 (8 hdr + 4 data)
    let udp_header = [
        0x30, 0x39, 0x00, 0x35, // Src 12345, Dst 53
        0x00, 0x0c, 0x00, 0x00, // Length 12
        0xde, 0xad, 0xbe, 0xef, // 4 payload bytes
    ];
    raw.extend_from_slice(&udp_header);

    let pkt = parser.parse_packet(&raw, 2000000).unwrap().expect("Failed to parse VLAN packet");

    assert_eq!(pkt.src_ip, "10.1.2.3".parse::<IpAddr>().unwrap());
    assert_eq!(pkt.dst_ip, "10.4.5.6".parse::<IpAddr>().unwrap());
    assert_eq!(pkt.src_port, 12345);
    assert_eq!(pkt.dst_port, 53);
    assert_eq!(pkt.protocol, 17);
    assert_eq!(pkt.header_bytes, 8);
    assert_eq!(pkt.payload_bytes, 4);
}

#[test]
fn test_parse_linux_sll_ipv6_tcp() {
    let mut parser = PacketParser::new(LINKTYPE_LINUX_SLL, true, true);

    let mut raw = Vec::new();
    // Linux SLL v1 header (16 bytes), last 2 bytes EtherType 0x86DD (IPv6)
    raw.extend_from_slice(&[0x00, 0x00, 0x03, 0x04, 0x00, 0x06, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]);
    raw.extend_from_slice(&[0x86, 0xdd]); // IPv6

    // IPv6 header: 40 bytes
    // Ver 6, Payload Length 24, Next Header 6 (TCP), Hop Limit 64
    let mut ipv6_header = [0u8; 40];
    ipv6_header[0] = 0x60;
    ipv6_header[4] = 0x00;
    ipv6_header[5] = 0x18; // 24 bytes payload
    ipv6_header[6] = 0x06; // Next Header = TCP
    ipv6_header[7] = 0x40;
    // Src: 2001:db8::1
    ipv6_header[8] = 0x20; ipv6_header[9] = 0x01; ipv6_header[10] = 0x0d; ipv6_header[11] = 0xb8;
    ipv6_header[23] = 0x01;
    // Dst: 2001:db8::2
    ipv6_header[24] = 0x20; ipv6_header[25] = 0x01; ipv6_header[26] = 0x0d; ipv6_header[27] = 0xb8;
    ipv6_header[39] = 0x02;
    raw.extend_from_slice(&ipv6_header);

    // TCP: Src 443, Dst 49152, Data Offset 5 (20 bytes), Flags ACK+PSH (0x18), 4 bytes data
    let tcp_header = [
        0x01, 0xbb, 0xc0, 0x00, // Src 443, Dst 49152
        0x00, 0x00, 0x10, 0x00,
        0x00, 0x00, 0x20, 0x00,
        0x50, 0x18, 0x0f, 0xa0, // Offset 5 (20 bytes), Flags PSH|ACK (0x18), Win 4000
        0x00, 0x00, 0x00, 0x00,
        0x01, 0x02, 0x03, 0x04, // 4 payload bytes
    ];
    raw.extend_from_slice(&tcp_header);

    let pkt = parser.parse_packet(&raw, 3000000).unwrap().expect("Failed to parse IPv6 TCP packet");

    assert_eq!(pkt.src_ip, "2001:db8::1".parse::<IpAddr>().unwrap());
    assert_eq!(pkt.dst_ip, "2001:db8::2".parse::<IpAddr>().unwrap());
    assert_eq!(pkt.src_port, 443);
    assert_eq!(pkt.dst_port, 49152);
    assert_eq!(pkt.protocol, 6);
    assert!(pkt.flag_psh);
    assert!(pkt.flag_ack);
    assert_eq!(pkt.header_bytes, 20);
    assert_eq!(pkt.payload_bytes, 4);
}
