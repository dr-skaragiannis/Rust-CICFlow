use cicflowmeter::engine::{EngineConfig, FlowEngine, OutputFormat};
use cicflowmeter::flow::feature::CIC_FEATURE_NAMES;
use std::fs::File;
use std::io::Write;
use tempfile::tempdir;

fn create_sample_pcap(path: &std::path::Path) {
    let mut file = File::create(path).unwrap();

    // PCAP Global Header (24 bytes)
    // Magic: 0xa1b2c3d4 (Big Endian)
    let global_header = [
        0xa1, 0xb2, 0xc3, 0xd4, // magic
        0x00, 0x02, // version major 2
        0x00, 0x04, // version minor 4
        0x00, 0x00, 0x00, 0x00, // thiszone
        0x00, 0x00, 0x00, 0x00, // sigfigs
        0x00, 0x00, 0xff, 0xff, // snaplen 65535
        0x00, 0x00, 0x00, 0x01, // linktype 1 (Ethernet)
    ];
    file.write_all(&global_header).unwrap();

    // Write 4 packets belonging to a TCP connection
    let packets = [
        // Packet 1: 192.168.1.1:10000 -> 192.168.1.2:80, SYN, ts=1.000000
        (1, 0, create_tcp_packet([192, 168, 1, 1], [192, 168, 1, 2], 10000, 80, 0x02, &[])),
        // Packet 2: 192.168.1.2:80 -> 192.168.1.1:10000, SYN-ACK, ts=1.010000
        (1, 10000, create_tcp_packet([192, 168, 1, 2], [192, 168, 1, 1], 80, 10000, 0x12, &[])),
        // Packet 3: 192.168.1.1:10000 -> 192.168.1.2:80, ACK+Data, ts=1.020000
        (1, 20000, create_tcp_packet([192, 168, 1, 1], [192, 168, 1, 2], 10000, 80, 0x18, &[0xaa; 64])),
        // Packet 4: 192.168.1.2:80 -> 192.168.1.1:10000, ACK+Data, ts=1.030000
        (1, 30000, create_tcp_packet([192, 168, 1, 2], [192, 168, 1, 1], 80, 10000, 0x18, &[0xbb; 128])),
    ];

    for (sec, usec, pkt_bytes) in packets {
        let mut pkt_header = [0u8; 16];
        pkt_header[0..4].copy_from_slice(&(sec as u32).to_be_bytes());
        pkt_header[4..8].copy_from_slice(&(usec as u32).to_be_bytes());
        pkt_header[8..12].copy_from_slice(&(pkt_bytes.len() as u32).to_be_bytes());
        pkt_header[12..16].copy_from_slice(&(pkt_bytes.len() as u32).to_be_bytes());

        file.write_all(&pkt_header).unwrap();
        file.write_all(&pkt_bytes).unwrap();
    }
}

fn create_tcp_packet(src_ip: [u8; 4], dst_ip: [u8; 4], sport: u16, dport: u16, flags: u8, payload: &[u8]) -> Vec<u8> {
    let mut raw = Vec::new();
    // Ethernet header: 14 bytes
    raw.extend_from_slice(&[0x00, 0x11, 0x22, 0x33, 0x44, 0x55]);
    raw.extend_from_slice(&[0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb]);
    raw.extend_from_slice(&[0x08, 0x00]); // IPv4

    // IPv4 header: 20 bytes
    let total_len = 20 + 20 + payload.len();
    raw.push(0x45); // Ver 4, IHL 5
    raw.push(0x00);
    raw.extend_from_slice(&(total_len as u16).to_be_bytes());
    raw.extend_from_slice(&[0x12, 0x34, 0x40, 0x00]);
    raw.push(64); // TTL
    raw.push(6);  // Proto TCP
    raw.extend_from_slice(&[0x00, 0x00]); // Checksum
    raw.extend_from_slice(&src_ip);
    raw.extend_from_slice(&dst_ip);

    // TCP header: 20 bytes
    raw.extend_from_slice(&sport.to_be_bytes());
    raw.extend_from_slice(&dport.to_be_bytes());
    raw.extend_from_slice(&[0x00, 0x00, 0x00, 0x01]); // Seq
    raw.extend_from_slice(&[0x00, 0x00, 0x00, 0x01]); // Ack
    raw.push(0x50); // Data offset 5 (20 bytes)
    raw.push(flags);
    raw.extend_from_slice(&[0x7f, 0xff]); // Window
    raw.extend_from_slice(&[0x00, 0x00]); // Checksum
    raw.extend_from_slice(&[0x00, 0x00]); // Urgent

    // Payload
    raw.extend_from_slice(payload);

    raw
}

#[test]
fn test_pcap_processing_to_csv() {
    let temp_dir = tempdir().unwrap();
    let pcap_file = temp_dir.path().join("sample.pcap");
    let out_dir = temp_dir.path().join("out");

    create_sample_pcap(&pcap_file);

    let config = EngineConfig {
        bidirectional: true,
        format: OutputFormat::Csv,
        label: Some("TestFlow".to_string()),
        min_packets_per_flow: 2,
        ..Default::default()
    };

    let engine = FlowEngine::new(config);
    let results = engine.process_path(&pcap_file, &out_dir).unwrap();

    assert_eq!(results.len(), 1);
    let (_path, stat) = &results[0];
    assert_eq!(stat.total_packets, 4);
    assert_eq!(stat.valid_packets, 4);
    assert_eq!(stat.total_flows, 1);

    let out_csv = out_dir.join("sample.pcap_Flow.csv");
    assert!(out_csv.exists());

    let content = std::fs::read_to_string(&out_csv).unwrap();
    let lines: Vec<&str> = content.lines().collect();

    assert_eq!(lines.len(), 2, "Should have 1 header line and 1 data line");

    let header = lines[0];
    let header_cols: Vec<&str> = header.split(',').collect();
    assert_eq!(header_cols.len(), 84);
    assert_eq!(header_cols[0], CIC_FEATURE_NAMES[0]);
    assert_eq!(header_cols[83], CIC_FEATURE_NAMES[83]);

    let data_row = lines[1];
    let cols: Vec<&str> = data_row.split(',').collect();
    assert_eq!(cols.len(), 84);
    assert_eq!(cols[1], "192.168.1.1"); // Src IP
    assert_eq!(cols[2], "10000");       // Src Port
    assert_eq!(cols[3], "192.168.1.2"); // Dst IP
    assert_eq!(cols[4], "80");          // Dst Port
    assert_eq!(cols[5], "6");           // Protocol
    assert_eq!(cols[8], "2");           // Total Fwd Packets
    assert_eq!(cols[9], "2");           // Total Bwd Packets
    assert_eq!(cols[10], "64.0");       // Total Length Fwd (packet 3 has 64 bytes)
    assert_eq!(cols[11], "128.0");      // Total Length Bwd (packet 4 has 128 bytes)
    assert_eq!(cols[83], "TestFlow");   // Label
}

#[test]
fn test_pcap_processing_to_json() {
    let temp_dir = tempdir().unwrap();
    let pcap_file = temp_dir.path().join("sample.pcap");
    let out_dir = temp_dir.path().join("out_json");

    create_sample_pcap(&pcap_file);

    let config = EngineConfig {
        bidirectional: true,
        format: OutputFormat::Json,
        label: Some("TestJson".to_string()),
        min_packets_per_flow: 2,
        ..Default::default()
    };

    let engine = FlowEngine::new(config);
    let results = engine.process_path(&pcap_file, &out_dir).unwrap();

    assert_eq!(results.len(), 1);

    let out_json = out_dir.join("sample.pcap_Flow.json");
    assert!(out_json.exists());

    let content = std::fs::read_to_string(&out_json).unwrap();
    let json_val: serde_json::Value = serde_json::from_str(&content).unwrap();

    assert!(json_val.is_array());
    let arr = json_val.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["src_ip"], "192.168.1.1");
    assert_eq!(arr[0]["dst_ip"], "192.168.1.2");
    assert_eq!(arr[0]["label"], "TestJson");
}
