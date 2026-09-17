#!/usr/bin/env bash
set -e

# ==============================================================================
# CICFlowMeter Performance & Correctness Replication Script
# Replicates the side-by-side benchmark between Legacy Java and Pure Rust
# ==============================================================================

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
PCAP_SAMPLE="$ROOT_DIR/tests/data/sample_traffic.pcap"
PCAP_REAL="$ROOT_DIR/tests/data/real_traffic.pcap"
OUT_DIR="/tmp/cicflowmeter_replication"

mkdir -p "$OUT_DIR/java" "$OUT_DIR/rust"

echo "================================================================="
echo " 1. Building Optimized Rust CICFlowMeter"
echo "================================================================="
cd "$ROOT_DIR"
cargo build --release

echo "================================================================="
echo " 2. Running Rust CICFlowMeter on Sample Dataset"
echo "================================================================="
"$ROOT_DIR/target/release/cicflowmeter" -r "$PCAP_SAMPLE" -o "$OUT_DIR/rust" --format csv -v

echo "================================================================="
echo " 3. Running Feature Parity Comparison (Java vs Rust)"
echo "================================================================="
if [ -d "/tmp/cicflowmeter-java" ]; then
    echo "Running Java CICFlowMeter..."
    JAVA_CP="target/classes:jnetpcap/linux/jnetpcap-1.4.r1425/jnetpcap.jar:$(cd /tmp/cicflowmeter-java && mvn dependency:build-classpath | grep -A 1 'Dependencies classpath:' | tail -n 1)"
    (cd /tmp/cicflowmeter-java && java -Djava.library.path=jnetpcap/linux/jnetpcap-1.4.r1425 -cp "$JAVA_CP" cic.cs.unb.ca.ifm.Cmd "$PCAP_SAMPLE" "$OUT_DIR/java")
    python3 "$ROOT_DIR/scripts/compare_outputs.py" "$OUT_DIR/java/sample_traffic.pcap_Flow.csv" "$OUT_DIR/rust/sample_traffic.pcap_Flow.csv"
else
    echo "Notice: Java reference directory /tmp/cicflowmeter-java not found, running Rust verification tests."
fi

echo "================================================================="
echo " 4. Running Full Cargo Test Suite"
echo "================================================================="
cargo test --all-targets

echo "================================================================="
echo " 5. Running Flow Benchmark Suite"
echo "================================================================="
cargo bench

echo "================================================================="
echo " Experiment Replication Successfully Completed!"
echo " Results available in $OUT_DIR"
echo "================================================================="
