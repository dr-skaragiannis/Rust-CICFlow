use cicflowmeter::engine::{EngineConfig, FlowEngine, OutputFormat};
use cicflowmeter::flow::basic_flow::BasicFlow;
use cicflowmeter::flow::generator::FlowGenerator;
use cicflowmeter::reader::PcapReader;
use std::env;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        println!("Usage: cargo run --example offline_analyzer <input.pcap>");
        return Ok(());
    }

    let input_path = Path::new(&args[1]);

    println!("==================================================================");
    println!(" CICFlowMeter Rust: Library Example (Offline Analyzer)");
    println!(" Analyzing: {}", input_path.display());
    println!("==================================================================");

    // Method 1: High-Level FlowEngine
    let config = EngineConfig {
        bidirectional: true,
        flow_timeout_us: 120_000_000,
        activity_timeout_us: 5_000_000,
        format: OutputFormat::Csv,
        label: Some("EXAMPLE_LABEL".to_string()),
        ..Default::default()
    };

    let engine = FlowEngine::new(config);
    let output_file = "example_flow_output.csv";
    let stats = engine.process_file(input_path, output_file)?;

    println!("\nEngine Process Results:");
    println!("  Total Packets Processed : {}", stats.total_packets);
    println!("  Valid Network Packets   : {}", stats.valid_packets);
    println!("  Extracted Flows         : {}", stats.total_flows);
    println!("  Elapsed Time            : {:.3} s", stats.elapsed_ms as f64 / 1000.0);
    println!("  Output Written To       : {}", output_file);

    // Method 2: Low-Level Streaming Flow Generator with Event Handlers
    println!("\nStreaming Low-Level Inspection (First 5 Flows):");
    let mut reader = PcapReader::open(input_path, true, true)?;
    let mut generator = FlowGenerator::new(true, 120_000_000, 5_000_000, None, false);

    let printed_flows = Arc::new(AtomicUsize::new(0));
    let p_cnt = printed_flows.clone();

    generator.set_flow_listener(move |flow: BasicFlow| {
        let current = p_cnt.fetch_add(1, Ordering::Relaxed);
        if current < 5 {
            let feat = flow.extract_features();
            println!(
                "  Flow [{}] {}:{} -> {}:{} (Proto: {}, Dur: {} µs, Pkts: {}, Bytes/s: {:.2})",
                current + 1,
                feat.src_ip,
                feat.src_port,
                feat.dst_ip,
                feat.dst_port,
                feat.protocol,
                feat.flow_duration,
                feat.tot_fwd_pkts + feat.tot_bwd_pkts,
                feat.flow_bytes_sec
            );
        }
    });

    while let Some(pkt) = reader.next_packet()? {
        generator.add_packet(pkt);
    }
    generator.finish_all_flows();

    println!("\nDone.");
    Ok(())
}
