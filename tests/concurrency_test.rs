use cicflowmeter::engine::{EngineConfig, FlowEngine, OutputFormat};
use std::fs::File;
use std::io::Write;
use std::path::Path;
use tempfile::tempdir;

fn create_dummy_pcap(path: &Path, flow_id_offset: u16) {
    let mut file = File::create(path).unwrap();

    let global_header = [
        0xa1, 0xb2, 0xc3, 0xd4,
        0x00, 0x02,
        0x00, 0x04,
        0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0xff, 0xff,
        0x00, 0x00, 0x00, 0x01,
    ];
    file.write_all(&global_header).unwrap();

    // 10 packets per pcap
    for i in 0..10 {
        let is_forward = i % 2 == 0;
        let (sip, dip, sport, dport) = if is_forward {
            ([10, 0, 0, 1], [10, 0, 0, 2], 20000 + flow_id_offset, 80)
        } else {
            ([10, 0, 0, 2], [10, 0, 0, 1], 80, 20000 + flow_id_offset)
        };

        let payload = vec![0x41 + (i as u8); 50 + (i as usize) * 10];
        let total_len = 20 + 20 + payload.len();

        let mut raw = Vec::new();
        // Ethernet
        raw.extend_from_slice(&[0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb, 0x08, 0x00]);
        // IPv4
        raw.push(0x45);
        raw.push(0x00);
        raw.extend_from_slice(&(total_len as u16).to_be_bytes());
        raw.extend_from_slice(&[0x12, 0x34, 0x40, 0x00]);
        raw.push(64);
        raw.push(6);
        raw.extend_from_slice(&[0x00, 0x00]);
        raw.extend_from_slice(&sip);
        raw.extend_from_slice(&dip);
        // TCP
        raw.extend_from_slice(&sport.to_be_bytes());
        raw.extend_from_slice(&dport.to_be_bytes());
        raw.extend_from_slice(&(i as u32 * 100).to_be_bytes());
        raw.extend_from_slice(&[0x00, 0x00, 0x00, 0x01]);
        raw.push(0x50);
        raw.push(0x18); // PSH+ACK
        raw.extend_from_slice(&[0x40, 0x00]);
        raw.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]);
        raw.extend_from_slice(&payload);

        let sec = 1000 + i as u32;
        let usec = 50000 * i as u32;

        let mut pkt_hdr = [0u8; 16];
        pkt_hdr[0..4].copy_from_slice(&sec.to_be_bytes());
        pkt_hdr[4..8].copy_from_slice(&usec.to_be_bytes());
        pkt_hdr[8..12].copy_from_slice(&(raw.len() as u32).to_be_bytes());
        pkt_hdr[12..16].copy_from_slice(&(raw.len() as u32).to_be_bytes());

        file.write_all(&pkt_hdr).unwrap();
        file.write_all(&raw).unwrap();
    }
}

#[test]
fn test_parallel_batch_processing() {
    let temp_dir = tempdir().unwrap();
    let in_dir = temp_dir.path().join("in_pcaps");
    let out_dir = temp_dir.path().join("out_flows");
    std::fs::create_dir_all(&in_dir).unwrap();

    // Create 10 different PCAP files
    for idx in 0..10 {
        let pcap_path = in_dir.join(format!("traffic_{:02}.pcap", idx));
        create_dummy_pcap(&pcap_path, idx as u16);
    }

    let config = EngineConfig {
        threads: 4,
        format: OutputFormat::Csv,
        label: Some("BatchFlow".to_string()),
        min_packets_per_flow: 2,
        ..Default::default()
    };

    let engine = FlowEngine::new(config);
    let results = engine.process_path(&in_dir, &out_dir).unwrap();

    assert_eq!(results.len(), 10, "Should process all 10 PCAP files");

    for (path, stat) in &results {
        assert_eq!(stat.total_packets, 10);
        assert_eq!(stat.valid_packets, 10);
        assert_eq!(stat.total_flows, 1);

        let file_name = path.file_name().unwrap().to_str().unwrap();
        let expected_csv = out_dir.join(format!("{}_Flow.csv", file_name));
        assert!(expected_csv.exists(), "CSV output file must exist: {:?}", expected_csv);

        let content = std::fs::read_to_string(&expected_csv).unwrap();
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines.len(), 2);
    }
}
