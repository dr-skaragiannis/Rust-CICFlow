#[cfg(feature = "live-capture")]
use cicflowmeter::flow::basic_flow::BasicFlow;
use cicflowmeter::flow::generator::FlowGenerator;
use cicflowmeter::reader::LiveCapture;
use std::env;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

fn main() -> anyhow::Result<()> {
    #[cfg(feature = "live-capture")]
    {
        let args: Vec<String> = env::args().collect();
        let interface_name = if args.len() > 1 {
            &args[1]
        } else {
            "eth0"
        };

        println!("==================================================================");
        println!(" CICFlowMeter Rust: Library Example (Live Sniffer)");
        println!(" Listening on Interface: {}", interface_name);
        println!(" Press Ctrl+C to terminate sniffer.");
        println!("==================================================================");

        let running = Arc::new(AtomicBool::new(true));
        let r = running.clone();

        ctrlc::set_handler(move || {
            println!("\nTerminating live sniffer...");
            r.store(false, Ordering::Relaxed);
        })?;

        let mut capture = match LiveCapture::new(
            interface_name,
            true,
            65535,
            1000,
            true,
            true,
            None,
        ) {
            Ok(cap) => cap,
            Err(e) => {
                eprintln!("Error opening interface {}: {}", interface_name, e);
                eprintln!("Tip: live packet capture usually requires root/sudo permissions.");
                return Ok(());
            }
        };

        let mut generator = FlowGenerator::new(true, 60_000_000, 5_000_000, Some("LIVE".to_string()), false);

        generator.set_flow_listener(|flow: BasicFlow| {
            let feat = flow.extract_features();
            println!(
                "🔥 [FLOW CLOSED] {} | Duration: {} µs | Packets: (Fwd: {}, Bwd: {}) | Bytes: {:.0}",
                feat.flow_id,
                feat.flow_duration,
                feat.tot_fwd_pkts,
                feat.tot_bwd_pkts,
                feat.tot_len_fwd_pkts + feat.tot_len_bwd_pkts
            );
        });

        let mut total_pkts = 0u64;

        while running.load(Ordering::Relaxed) {
            if let Some(pkt) = capture.next_packet()? {
                total_pkts += 1;
                generator.add_packet(pkt);

                if total_pkts % 100 == 0 {
                    print!("\rPackets Captured: {} | Active Flow Table Size: {}  ", total_pkts, generator.active_flow_count());
                    std::io::Write::flush(&mut std::io::stdout())?;
                }
            }
        }

        println!("\nFlushing remaining active flows...");
        let remaining = generator.finish_all_flows();
        println!("Captured {} total packets, generated {} total flows.", total_pkts, generator.total_finished_flows() + remaining.len() as u64);
    }

    #[cfg(not(feature = "live-capture"))]
    {
        println!("Live capture feature is disabled.");
    }

    Ok(())
}
