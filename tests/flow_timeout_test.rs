use cicflowmeter::flow::generator::FlowGenerator;
use cicflowmeter::packet::info::BasicPacketInfo;
use std::net::IpAddr;

#[test]
fn test_flow_timeout_expiration() {
    let mut gen = FlowGenerator::new(true, 10_000_000, 5_000_000, Some("TimeoutTest".to_string()), false);

    let src_ip: IpAddr = "10.0.0.1".parse().unwrap();
    let dst_ip: IpAddr = "10.0.0.2".parse().unwrap();
    let src_bytes = vec![10, 0, 0, 1];
    let dst_bytes = vec![10, 0, 0, 2];

    // Flow 1: Packet 1 (t=0) and Packet 2 (t=1,000,000 µs = 1s)
    let p1 = BasicPacketInfo::new(
        1, src_ip, dst_ip, src_bytes.clone(), dst_bytes.clone(), 1234, 80, 6, 0, 50, 20, 1000,
        (false, false, false, false, true, false, false, false),
    );
    let p2 = BasicPacketInfo::new(
        2, dst_ip, src_ip, dst_bytes.clone(), src_bytes.clone(), 80, 1234, 6, 1_000_000, 50, 20, 1000,
        (false, false, false, false, true, false, false, false),
    );
    gen.add_packet(p1);
    gen.add_packet(p2);

    assert_eq!(gen.total_finished_flows(), 0);
    assert_eq!(gen.active_flow_count(), 1);

    // Packet 3 arrives at t=15,000,000 µs (15s > 10s flow_timeout!)
    // This should trigger completion of Flow 1 and start of Flow 2
    let p3 = BasicPacketInfo::new(
        3, src_ip, dst_ip, src_bytes.clone(), dst_bytes.clone(), 1234, 80, 6, 15_000_000, 50, 20, 1000,
        (false, false, false, false, true, false, false, false),
    );
    gen.add_packet(p3);

    assert_eq!(gen.total_finished_flows(), 1);
    assert_eq!(gen.active_flow_count(), 1);

    let finished = gen.get_finished_flows();
    assert_eq!(finished.len(), 1);
    assert_eq!(finished[0].flow_duration(), 1_000_000);
    assert_eq!(finished[0].packet_count(), 2);

    // Add packet 4 for Flow 2 (t=16,000,000 µs)
    let p4 = BasicPacketInfo::new(
        4, dst_ip, src_ip, dst_bytes.clone(), src_bytes.clone(), 80, 1234, 6, 16_000_000, 50, 20, 1000,
        (false, false, false, false, true, false, false, false),
    );
    gen.add_packet(p4);

    let remaining = gen.finish_all_flows();
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].flow_duration(), 1_000_000);
    assert_eq!(remaining[0].flow_start_time, 15_000_000);
}

#[test]
fn test_fin_flag_termination() {
    let mut gen = FlowGenerator::new(true, 120_000_000, 5_000_000, Some("FinTest".to_string()), false);

    let src_ip: IpAddr = "10.0.0.1".parse().unwrap();
    let dst_ip: IpAddr = "10.0.0.2".parse().unwrap();
    let src_bytes = vec![10, 0, 0, 1];
    let dst_bytes = vec![10, 0, 0, 2];

    // Handshake
    let p1 = BasicPacketInfo::new(1, src_ip, dst_ip, src_bytes.clone(), dst_bytes.clone(), 1234, 80, 6, 1_000_000, 0, 20, 1000, (false, true, false, false, false, false, false, false));
    let p2 = BasicPacketInfo::new(2, dst_ip, src_ip, dst_bytes.clone(), src_bytes.clone(), 80, 1234, 6, 1_010_000, 0, 20, 1000, (false, true, false, false, true, false, false, false));
    gen.add_packet(p1);
    gen.add_packet(p2);

    // Forward FIN
    let p3 = BasicPacketInfo::new(3, src_ip, dst_ip, src_bytes.clone(), dst_bytes.clone(), 1234, 80, 6, 1_020_000, 0, 20, 1000, (true, false, false, false, true, false, false, false));
    gen.add_packet(p3);

    // Backward flow not yet finished
    assert_eq!(gen.total_finished_flows(), 0);
    assert_eq!(gen.active_flow_count(), 1);

    // Backward FIN -> should finish flow
    let p4 = BasicPacketInfo::new(4, dst_ip, src_ip, dst_bytes.clone(), src_bytes.clone(), 80, 1234, 6, 1_030_000, 0, 20, 1000, (true, false, false, false, true, false, false, false));
    gen.add_packet(p4);

    assert_eq!(gen.total_finished_flows(), 1);
    assert_eq!(gen.active_flow_count(), 0);

    let finished = gen.get_finished_flows();
    assert_eq!(finished.len(), 1);
    assert_eq!(finished[0].packet_count(), 4);
    assert_eq!(finished[0].flag_fin, 2);
}

#[test]
fn test_rst_flag_termination() {
    let mut gen = FlowGenerator::new(true, 120_000_000, 5_000_000, Some("RstTest".to_string()), false);

    let src_ip: IpAddr = "10.0.0.1".parse().unwrap();
    let dst_ip: IpAddr = "10.0.0.2".parse().unwrap();
    let src_bytes = vec![10, 0, 0, 1];
    let dst_bytes = vec![10, 0, 0, 2];

    let p1 = BasicPacketInfo::new(1, src_ip, dst_ip, src_bytes.clone(), dst_bytes.clone(), 1234, 80, 6, 1_000_000, 0, 20, 1000, (false, true, false, false, false, false, false, false));
    let p2 = BasicPacketInfo::new(2, dst_ip, src_ip, dst_bytes.clone(), src_bytes.clone(), 80, 1234, 6, 1_010_000, 0, 20, 1000, (false, true, false, false, true, false, false, false));
    gen.add_packet(p1);
    gen.add_packet(p2);

    // RST packet from server
    let p3 = BasicPacketInfo::new(3, dst_ip, src_ip, dst_bytes.clone(), src_bytes.clone(), 80, 1234, 6, 1_020_000, 0, 20, 1000, (false, false, true, false, false, false, false, false));
    gen.add_packet(p3);

    assert_eq!(gen.total_finished_flows(), 1);
    assert_eq!(gen.active_flow_count(), 0);

    let finished = gen.get_finished_flows();
    assert_eq!(finished.len(), 1);
    assert_eq!(finished[0].packet_count(), 3);
    assert_eq!(finished[0].flag_rst, 1);
}

#[test]
fn test_bulk_transfer_calculation() {
    let src_ip: IpAddr = "192.168.1.5".parse().unwrap();
    let dst_ip: IpAddr = "192.168.1.6".parse().unwrap();
    let src_bytes = vec![192, 168, 1, 5];
    let dst_bytes = vec![192, 168, 1, 6];

    let p1 = BasicPacketInfo::new(1, src_ip, dst_ip, src_bytes.clone(), dst_bytes.clone(), 4000, 80, 6, 1_000_000, 100, 20, 1000, (false, false, false, false, true, false, false, false));
    let mut flow = cicflowmeter::flow::basic_flow::BasicFlow::new(&p1, true, 5_000_000, None, false);

    // Send 5 forward packets in rapid succession (< 1.0s gap) with 100 bytes each
    for i in 2..=6 {
        let p = BasicPacketInfo::new(
            i,
            src_ip,
            dst_ip,
            src_bytes.clone(),
            dst_bytes.clone(),
            4000,
            80,
            6,
            1_000_000 + (i as i64) * 10_000, // 10ms gap
            100,
            20,
            1000,
            (false, false, false, false, true, false, false, false),
        );
        flow.add_packet(&p);
    }

    let features = flow.extract_features();
    // 6 packets total. 1st packet was helper, bulk triggers at 4th helper packet (which is pkt 4), then 5 and 6 continue bulk
    assert!(features.fwd_bytes_bulk_avg > 0, "Fwd Bytes/Bulk Avg should be > 0");
    assert!(features.fwd_pkt_bulk_avg >= 4, "Fwd Pkts/Bulk Avg should be >= 4");
}
