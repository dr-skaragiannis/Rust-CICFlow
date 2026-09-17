use cicflowmeter::flow::generator::FlowGenerator;
use cicflowmeter::packet::info::BasicPacketInfo;
use cicflowmeter::packet::parser::PacketParser;
use std::net::IpAddr;
use std::time::Instant;

fn main() {
    println!("==================================================================");
    println!(" CICFlowMeter Rust: Throughput Benchmark");
    println!("==================================================================");

    // 1. Packet Parser Throughput Benchmark
    let mut parser = PacketParser::new(1, true, true);
    let mut raw = Vec::new();
    raw.extend_from_slice(&[0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb, 0x08, 0x00]);
    raw.extend_from_slice(&[0x45, 0x00, 0x00, 0x3c, 0x12, 0x34, 0x40, 0x00, 64, 6, 0, 0, 192, 168, 1, 10, 10, 0, 0, 1]);
    raw.extend_from_slice(&[0xd4, 0x31, 0x00, 0x50, 0, 0, 0, 1, 0, 0, 0, 1, 0x50, 0x18, 0x40, 0x00, 0, 0, 0, 0]);
    raw.extend_from_slice(&[0x41; 20]);

    let parse_iters = 500_000;
    let start_parse = Instant::now();
    for i in 0..parse_iters {
        let _ = parser.parse_packet(&raw, i as i64 * 10);
    }
    let parse_time = start_parse.elapsed();
    let parse_pps = (parse_iters as f64) / parse_time.as_secs_f64();
    println!("1. Zero-Copy Packet Parser:");
    println!("   Parsed {} packets in {:.3} s -> {:.0} pkts/sec", parse_iters, parse_time.as_secs_f64(), parse_pps);

    // 2. Flow Generation & Statistical Aggregation Benchmark
    let mut generator = FlowGenerator::new(true, 120_000_000, 5_000_000, None, false);
    let src_ip: IpAddr = "192.168.1.10".parse().unwrap();
    let dst_ip: IpAddr = "10.0.0.1".parse().unwrap();
    let src_bytes = vec![192, 168, 1, 10];
    let dst_bytes = vec![10, 0, 0, 1];

    let flow_iters = 500_000;
    let start_flow = Instant::now();
    for i in 0..flow_iters {
        let is_fwd = (i % 2) == 0;
        let flow_slot = (i / 100) % 500; // 500 concurrent active flows
        let sport = (20000 + flow_slot) as u16;

        let p = if is_fwd {
            BasicPacketInfo::new(
                i as u64,
                src_ip,
                dst_ip,
                src_bytes.clone(),
                dst_bytes.clone(),
                sport,
                80,
                6,
                i as i64 * 100,
                500,
                20,
                65535,
                (false, false, false, true, true, false, false, false),
            )
        } else {
            BasicPacketInfo::new(
                i as u64,
                dst_ip,
                src_ip,
                dst_bytes.clone(),
                src_bytes.clone(),
                80,
                sport,
                6,
                i as i64 * 100 + 50,
                800,
                20,
                65535,
                (false, false, false, true, true, false, false, false),
            )
        };
        generator.add_packet(p);
    }
    let _ = generator.finish_all_flows();
    let flow_time = start_flow.elapsed();
    let flow_pps = (flow_iters as f64) / flow_time.as_secs_f64();
    println!("\n2. Flow Table Tracking & 84-Feature Aggregation:");
    println!("   Processed {} packets across 500 concurrent flows in {:.3} s -> {:.0} pkts/sec", flow_iters, flow_time.as_secs_f64(), flow_pps);

    println!("==================================================================");
}
