# Evaluation — CICFlow vs Rust-CICFlow

Host: **Local_Computer** · Run: **2026-09-23 02:01:55**


## Methodology


1. **Extractors under test**
   - **CICFlow (Python):** the `cicflowmeter` pip package (uehara implementation), driven through its public
     `FlowSession` API by `scripts/pcmeter_driver.py` (required because the package CLI needs `tcpdump` for
     offline PCAPs and its `toPacketList` finish hook references a scapy API removed in scapy >= 2.5).
   - **Rust-CICFlow:** the optimised `cicflowmeter` release binary built from this repository
     (`cargo build --release`, `--no-default-features` on Windows where libwpcap is absent).
2. **Configuration:** flow timeout 120 s (`120000000` µs), activity timeout 5 s (`5000000` µs),
   min packets per flow 2, CSV export, single worker thread. These match the canonical CIC-IDS settings.
3. **Trials:** every dataset is processed `N` times per extractor (N = configurable `--reps`); a warm-up
   run is executed and discarded before measurement. Reported metrics are the mean across trials.
4. **Instrumentation:** each extractor runs as an isolated subprocess. Wall-clock time is measured with a
   monotonic clock around the subprocess lifetime. Peak Resident Set Size is sampled every ~5 ms (including
   child processes) via `psutil`.
5. **Throughput** = packets in capture ÷ wall-clock time.
6. **Feature parity:** flow CSVs are reconciled on the canonical bidirectional 5-tuple
   (sorted IP pair, sorted port pair, protocol). For every matched flow and every comparable feature,
   Mean Absolute Error (MAE), maximum absolute difference and Pearson correlation are computed.
   *Status*: EXACT if max|diff| < 1e-6, CONCORDANT if r > 0.999, else DISCREPANCY.


## Environment

| Attribute | Value |
|---|---|
| Host | Local_Computer |
| OS / Platform | Windows 10 (AMD64) |
| Processor | AMD64 Family 25 Model 68 Stepping 1, AuthenticAMD |
| Python | 3.11.9 |
| Rust toolchain | cargo 1.98.0 (797e8a9bc 2026-08-05) |
| CICFlow (Python) package | 0.2.0 |
| Scapy | 2.6.1 |
| Pandas / Numpy | 3.0.2 / 1.26.4 |

## Limitations & notes

- The pip `cicflowmeter` flow extractor only handles IPv4 over Ethernet; packets using IPv6 or non-Ethernet encapsulations (e.g. `LINUX_SLL2`, present in `real_traffic.pcap`) are silently skipped by the Python extractor but processed by Rust-CICFlow. Flow counts therefore differ by design on such captures.
- The two extractors use different flow-termination logic (the Python implementation splits a bidirectional session on FIN and uses a 120 s sliding expiry; the Rust engine tracks a symmetric FIN state machine), so flow segmentation can differ even when every packet is seen.
- `wireshark_http.pcap` contains a single packet; both extractors therefore emit no meaningful flows under the default minimum-packets setting.
- Peak RSS is sampled at ~5 ms granularity; short-lived runs can slightly under-report the true instantaneous peak.
- The Python package stores the frame EtherType (2048 for IPv4) in its `protocol` column rather than the IP L4 protocol number; feature reconciliation resolves each Python flow to a Rust flow sharing the same IP/port pair and adopts that flow's protocol.
- The Python package reports `cwr_flag_count` as a copy of `fwd_urg_flags` (an upstream quirk) and `fwd/bwd_seg_size_avg` as copies of the packet-length means; the comparison maps upstream feature names onto the canonical 84-name schema where a clear semantic equivalent exists.

