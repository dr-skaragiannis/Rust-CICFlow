use cicflowmeter::flow::basic_flow::BasicFlow;
use cicflowmeter::packet::info::BasicPacketInfo;
use std::net::IpAddr;

#[test]
fn test_flow_features_calculation() {
    let src_ip: IpAddr = "192.168.1.10".parse().unwrap();
    let dst_ip: IpAddr = "192.168.1.20".parse().unwrap();
    let src_bytes = vec![192, 168, 1, 10];
    let dst_bytes = vec![192, 168, 1, 20];

    // Packet 1: Forward SYN packet (timestamp: 1,000,000 µs = 1.0 s)
    let p1 = BasicPacketInfo::new(
        1,
        src_ip,
        dst_ip,
        src_bytes.clone(),
        dst_bytes.clone(),
        50000,
        80,
        6,
        1_000_000,
        0,  // 0 payload bytes
        20, // 20 header bytes
        65535,
        (false, true, false, false, false, false, false, false), // SYN
    );

    let mut flow = BasicFlow::new(&p1, true, 5_000_000, Some("BENIGN"), false);

    assert_eq!(flow.packet_count(), 1);
    assert_eq!(flow.fwd_packet_count, 1);
    assert_eq!(flow.bwd_packet_count, 0);
    assert_eq!(flow.flag_syn, 1);
    assert_eq!(flow.init_win_bytes_forward, 65535);

    // Packet 2: Backward SYN-ACK packet (timestamp: 1,050,000 µs = 1.05 s)
    let p2 = BasicPacketInfo::new(
        2,
        dst_ip,
        src_ip,
        dst_bytes.clone(),
        src_bytes.clone(),
        80,
        50000,
        6,
        1_050_000,
        0,
        20,
        32768,
        (false, true, false, false, true, false, false, false), // SYN + ACK
    );
    flow.add_packet(&p2);

    assert_eq!(flow.packet_count(), 2);
    assert_eq!(flow.fwd_packet_count, 1);
    assert_eq!(flow.bwd_packet_count, 1);
    assert_eq!(flow.flag_syn, 2);
    assert_eq!(flow.flag_ack, 1);
    assert_eq!(flow.init_win_bytes_backward, 32768);

    // Packet 3: Forward Data ACK packet (timestamp: 1,100,000 µs = 1.10 s) with 100 bytes payload
    let p3 = BasicPacketInfo::new(
        3,
        src_ip,
        dst_ip,
        src_bytes.clone(),
        dst_bytes.clone(),
        50000,
        80,
        6,
        1_100_000,
        100,
        20,
        65535,
        (false, false, false, true, true, false, false, false), // PSH + ACK
    );
    flow.add_packet(&p3);

    // Packet 4: Backward Data packet (timestamp: 1,200,000 µs = 1.20 s) with 500 bytes payload
    let p4 = BasicPacketInfo::new(
        4,
        dst_ip,
        src_ip,
        dst_bytes.clone(),
        src_bytes.clone(),
        80,
        50000,
        6,
        1_200_000,
        500,
        20,
        32768,
        (false, false, false, true, true, false, false, false), // PSH + ACK
    );
    flow.add_packet(&p4);

    let features = flow.extract_features();

    // Verify Duration: 1,200,000 - 1,000,000 = 200,000 µs
    assert_eq!(features.flow_duration, 200_000);
    assert_eq!(features.tot_fwd_pkts, 2);
    assert_eq!(features.tot_bwd_pkts, 2);
    assert_eq!(features.tot_len_fwd_pkts, 100.0);
    assert_eq!(features.tot_len_bwd_pkts, 500.0);

    // Forward packet lengths: [0, 100] -> Max 100, Min 0, Mean 50
    assert_eq!(features.fwd_pkt_len_max, 100.0);
    assert_eq!(features.fwd_pkt_len_min, 0.0);
    assert_eq!(features.fwd_pkt_len_mean, 50.0);

    // Backward packet lengths: [0, 500] -> Max 500, Min 0, Mean 250
    assert_eq!(features.bwd_pkt_len_max, 500.0);
    assert_eq!(features.bwd_pkt_len_min, 0.0);
    assert_eq!(features.bwd_pkt_len_mean, 250.0);

    // Total bytes: 100 + 500 = 600 bytes in 0.2 s -> 3000 bytes/s
    assert!((features.flow_bytes_sec - 3000.0).abs() < 1e-6);
    // Total packets: 4 in 0.2 s -> 20 pkts/s
    assert!((features.flow_pkts_sec - 20.0).abs() < 1e-6);

    // Forward IAT: interval between p1 (1.0) and p3 (1.1) = 100,000 µs
    assert_eq!(features.fwd_iat_tot, 100_000.0);
    assert_eq!(features.fwd_iat_mean, 100_000.0);

    // Backward IAT: interval between p2 (1.05) and p4 (1.20) = 150,000 µs
    assert_eq!(features.bwd_iat_tot, 150_000.0);
    assert_eq!(features.bwd_iat_mean, 150_000.0);

    // Flags: FIN=0, SYN=2, RST=0, PSH=2, ACK=3
    assert_eq!(features.fin_cnt, 0);
    assert_eq!(features.syn_cnt, 2);
    assert_eq!(features.rst_cnt, 0);
    assert_eq!(features.psh_cnt, 2);
    assert_eq!(features.ack_cnt, 3);

    // Down/Up Ratio: 2 bwd / 2 fwd = 1.0
    assert_eq!(features.down_up_ratio, 1.0);

    // Average Packet Size: (0 + 0 + 100 + 500) / 4 = 150.0
    assert_eq!(features.pkt_size_avg, 150.0);

    // Init Win Bytes
    assert_eq!(features.fwd_win_byt, 65535);
    assert_eq!(features.bwd_win_byt, 32768);

    // Active Data Packets (fwd payload >= 1): 1 packet (p3 has 100 bytes)
    assert_eq!(features.fwd_act_pkt, 1);

    // Label
    assert_eq!(features.label, "BENIGN");

    // CSV Row serialization
    let csv_row = features.to_csv_row();
    let fields: Vec<&str> = csv_row.split(',').collect();
    assert_eq!(fields.len(), 84, "CSV row must contain exactly 84 columns");
}
