use clap::Parser;
use cicflowmeter::engine::{EngineConfig, FlowEngine, OutputFormat};
#[cfg(feature = "live-capture")]
use cicflowmeter::reader::LiveCapture;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

#[derive(Parser, Debug)]
#[command(
    name = "cicflowmeter",
    author = "CICFlowMeter Rust Contributors",
    version = env!("CARGO_PKG_VERSION"),
    about = "High-performance, memory-safe network traffic flow feature generator and PCAP analyzer in Rust",
    long_about = "A ground-up pure Rust implementation of CICFlowMeter for high-speed, zero-crash network flow feature extraction. Extracts 84 bidirectional statistical features from live network interfaces or PCAP/PCAPNG files for machine learning and intrusion detection datasets."
)]
struct Args {
    /// Input PCAP/PCAPNG file path or directory of PCAP files
    #[arg(short = 'r', long = "read")]
    read: Option<PathBuf>,

    /// Output directory for generated flow feature files
    #[arg(short = 'o', long = "out-dir", default_value = "./output")]
    out_dir: PathBuf,

    /// Live network interface to capture from (e.g., eth0, wlan0, any)
    #[arg(short = 'i', long = "interface")]
    interface: Option<String>,

    /// Flow timeout in microseconds (default: 120,000,000 µs = 120 seconds)
    #[arg(short = 'f', long = "flow-timeout", default_value = "120000000")]
    flow_timeout: i64,

    /// Activity timeout in microseconds (default: 5,000,000 µs = 5 seconds)
    #[arg(short = 'a', long = "activity-timeout", default_value = "5000000")]
    activity_timeout: i64,

    /// Output format: csv, json, or jsonl
    #[arg(long = "format", default_value = "csv")]
    format: OutputFormat,

    /// Number of worker threads for parallel batch PCAP processing
    #[arg(short = 't', long = "threads", default_value_t = rayon::current_num_threads())]
    threads: usize,

    /// Dataset label to assign to flows
    #[arg(long = "label", default_value = "NeedManualLabel")]
    label: String,

    /// Minimum packets per flow to export (default: 2)
    #[arg(long = "min-packets", default_value = "2")]
    min_packets: u64,

    /// Berkeley Packet Filter (BPF) string for filtering live packets (e.g. "tcp and port 80")
    #[arg(long = "bpf")]
    bpf: Option<String>,

    /// Enable legacy Java CICFlowMeter compatibility mode (replicates legacy byte comparison and counter quirks)
    #[arg(long = "compat")]
    compat: bool,

    /// Disable IPv4 packet parsing
    #[arg(long = "no-ipv4")]
    no_ipv4: bool,

    /// Disable IPv6 packet parsing
    #[arg(long = "no-ipv6")]
    no_ipv6: bool,

    /// List all available network capture interfaces and exit
    #[arg(short = 'l', long = "list-interfaces")]
    list_interfaces: bool,

    /// Enable verbose / debug logging
    #[arg(short = 'v', long = "verbose")]
    verbose: bool,

    /// Suppress progress output and informational logs
    #[arg(short = 'q', long = "quiet")]
    quiet: bool,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    // Initialize logger
    let log_level = if args.quiet {
        log::LevelFilter::Off
    } else if args.verbose {
        log::LevelFilter::Debug
    } else {
        log::LevelFilter::Info
    };

    env_logger::Builder::new()
        .filter_level(log_level)
        .format_timestamp_millis()
        .init();

    #[cfg(feature = "live-capture")]
    if args.list_interfaces {
        println!("Available network capture interfaces:");
        match LiveCapture::list_devices() {
            Ok(devices) => {
                for (idx, dev) in devices.iter().enumerate() {
                    println!("  [{}] {}", idx + 1, dev);
                }
            }
            Err(e) => {
                eprintln!("Error listing interfaces: {}", e);
            }
        }
        return Ok(());
    }

    #[cfg(not(feature = "live-capture"))]
    if args.list_interfaces {
        eprintln!("Live capture feature was disabled at compile time.");
        return Ok(());
    }

    let config = EngineConfig {
        bidirectional: true,
        flow_timeout_us: args.flow_timeout,
        activity_timeout_us: args.activity_timeout,
        read_ipv4: !args.no_ipv4,
        read_ipv6: !args.no_ipv6,
        label: Some(args.label),
        compat_mode: args.compat,
        min_packets_per_flow: args.min_packets,
        format: args.format,
        threads: args.threads,
        show_progress: !args.quiet,
    };

    let engine = FlowEngine::new(config);

    if let Some(ref interface_name) = args.interface {
        #[cfg(feature = "live-capture")]
        {
            let running = Arc::new(AtomicBool::new(true));
            let r_ctrlc = running.clone();

            ctrlc::set_handler(move || {
                println!("\nReceived Ctrl+C, terminating capture gracefully...");
                r_ctrlc.store(false, Ordering::Relaxed);
            }).unwrap_or_else(|e| eprintln!("Warning: failed to set Ctrl+C handler: {}", e));

            let out_file_name = match args.format {
                OutputFormat::Csv => format!("{}_live_Flow.csv", interface_name),
                OutputFormat::Json => format!("{}_live_Flow.json", interface_name),
                OutputFormat::Jsonl => format!("{}_live_Flow.jsonl", interface_name),
            };
            let out_file = args.out_dir.join(out_file_name);

            if !args.quiet {
                println!("╔═════════════════════════════════════════════════════════════════╗");
                println!("║                 CICFlowMeter Rust Live Capture                  ║");
                println!("╚═════════════════════════════════════════════════════════════════╝");
                println!("Capturing on interface : {}", interface_name);
                println!("Output file            : {}", out_file.display());
                println!("Flow timeout           : {} µs ({} s)", args.flow_timeout, args.flow_timeout / 1_000_000);
                println!("Activity timeout       : {} µs ({} s)", args.activity_timeout, args.activity_timeout / 1_000_000);
                println!("Press Ctrl+C to stop.\n");
            }

            let stats = engine.process_live(
                interface_name,
                &out_file,
                args.bpf.as_deref(),
                running,
            )?;

            if !args.quiet {
                println!("\nLive capture finished.");
                println!("Total packets processed : {}", stats.total_packets);
                println!("Valid packets           : {}", stats.valid_packets);
                println!("Total flows generated   : {}", stats.total_flows);
                println!("Duration                : {:.2} s", stats.elapsed_ms as f64 / 1000.0);
            }
            return Ok(());
        }

        #[cfg(not(feature = "live-capture"))]
        {
            eprintln!("Live capture is not enabled in this build.");
            std::process::exit(1);
        }
    }

    if let Some(ref pcap_path) = args.read {
        if !args.quiet {
            println!("╔═════════════════════════════════════════════════════════════════╗");
            println!("║             CICFlowMeter Rust Offline PCAP Analyzer             ║");
            println!("╚═════════════════════════════════════════════════════════════════╝");
            println!("Input path       : {}", pcap_path.display());
            println!("Output directory : {}", args.out_dir.display());
            println!("Output format    : {:?}", args.format);
            println!("Worker threads   : {}", args.threads);
            println!("Flow timeout     : {} µs ({} s)", args.flow_timeout, args.flow_timeout / 1_000_000);
            println!("Activity timeout : {} µs ({} s)", args.activity_timeout, args.activity_timeout / 1_000_000);
            println!("-----------------------------------------------------------------");
        }

        let start = std::time::Instant::now();
        let results = engine.process_path(pcap_path, &args.out_dir)?;
        let total_time = start.elapsed();

        let mut total_pkts = 0u64;
        let mut total_valid = 0u64;
        let mut total_flows = 0u64;

        for (path, stat) in &results {
            total_pkts += stat.total_packets;
            total_valid += stat.valid_packets;
            total_flows += stat.total_flows;

            if !args.quiet && results.len() == 1 {
                let file_name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
                println!("File: {}", file_name);
                println!("  Total packets : {}", stat.total_packets);
                println!("  Valid packets : {}", stat.valid_packets);
                println!("  Total flows   : {}", stat.total_flows);
                println!("  Time elapsed  : {:.3} s", stat.elapsed_ms as f64 / 1000.0);
                if stat.elapsed_ms > 0 {
                    let pps = (stat.total_packets as f64) / (stat.elapsed_ms as f64 / 1000.0);
                    println!("  Throughput    : {:.0} pkts/sec", pps);
                }
            }
        }

        if !args.quiet {
            println!("=================================================================");
            println!("Summary:");
            println!("  Files processed : {}", results.len());
            println!("  Total packets   : {}", total_pkts);
            println!("  Valid packets   : {}", total_valid);
            println!("  Total flows     : {}", total_flows);
            println!("  Total time      : {:.3} s", total_time.as_secs_f64());
            if total_time.as_secs_f64() > 0.0 {
                let overall_pps = (total_pkts as f64) / total_time.as_secs_f64();
                println!("  Avg Throughput  : {:.0} pkts/sec", overall_pps);
            }
            println!("=================================================================");
        }

        return Ok(());
    }

    eprintln!("Error: Please provide either -r/--read <path> or -i/--interface <name> (or use -h for help).");
    std::process::exit(1);
}
