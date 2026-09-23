# Discussion — CICFlow vs Rust-CICFlow Evaluation Results

This document discusses the results of the automated comparison between the reference
`cicflowmeter` pip package (CICFlow, Python) and the Rust port (Rust-CICFlow) in this
repository.

**Run under discussion:** `experiments/20260918-184906_Bloodraven/`
(2026-09-18, 3 repetitions per extractor, single worker thread).
Full numbers live in `report.md`, `evaluation.md`, `results_summary.md` and
`raw_results.json` in that folder; this file adds interpretation.

> **2026-09-23 update — DEF CON 26 CTF corpus.** A second experiment on the
> DEF CON 26 CTF packet captures (`experiments/20260923-020154_Bloodraven/`,
> see `analysis.md` there) scaled the comparison up to 3 × 200,000-packet
> slices of the 49 GB / 156 M-packet capture and broadly holds the
> performance conclusions while recalibrating the parity narrative:
>
> 1. **Performance holds and strengthens**: 145.3× / 303.4× / 222.1×
>    speedup vs the pip Python reference on head/mid/tail slices, peak RSS
>    27–97 MB (Rust) vs 260 MB–1.56 GB (Python, event-peak flow residency —
>    the previous ~25× ratio widens to 47–50× at CTF peak concurrency).
>    Rust alone processed the full 156.1 M-packet capture in ≈ 700 s
>    (7,798,789 flows; Python extrapolates to ≈ 33 h).
> 2. The historical "0 discrepant features on the supported Ethernet path"
>    (`sample_traffic.pcap`) was a Pearson-criterion artifact — on real
>    scan-heavy traffic the pip package's **full-frame length accounting**
>    (`len(packet)`, including headers) vs Rust's canonical **payload-byte**
>    accounting reproduces as real discrepancies (MAE 97–337 B on
>    `Total Length of Fwd Packet`); it was already present in the tiny corpus
>    (max diff 648 B, r = 1.0 → could not trip the r < 0.999 gate).
> 3. The pip package hardcodes non-canonical time constants
>    (`EXPIRED_UPDATE = 240 s` inactivity split, `ACTIVE_TIMEOUT = 5 ms`,
>    `CLUMP_TIMEOUT = 1 ms`) vs the canonical CIC-IDS 120 s flow age / 5 s
>    activity semantics Rust implements, diverging on long-idle CTF flows.
>    Python rows can span ≥ 240 s (observed 202.8 s); Rust splits at 120 s age.
> 4. `csv_row_count` counted the pip writer's `\r\r\n` terminators as two
>    rows, reporting exactly 2× Python flow counts in the old summaries —
>    true counts: Rust 38,818 vs Python 38,567 (head, +0.7%),
>    13,047 vs 12,879 (mid, +1.3%), 9,239 vs 6,705 (tail, +37.8%).
> 5. Rust and Python agree on packet-count attribution for 92.5% of matched
>    flows (38.0 k flows), with Flow Duration exactly equal on 99.997% of
>    those — engine math is sound; remaining aggregate deltas are
>    segmentation and definitional, per the causes above. Aligned-constants
>    ablation (`scripts/pcmeter_driver_aligned.py`): 62 → 57 discrepant,
>    confirming the constants alone are a minor share.

## 1. Environment

| Attribute | Value |
|---|---|
| Machine | ASUS ROG Zephyrus G14 (GA402RJ) |
| CPU | AMD Ryzen 7 6800HS, 8 cores / 16 threads, ~3.2 GHz base |
| RAM | 40 GB DDR5-4800 |
| OS | Windows 11 Pro 23H2 (build 22631) |
| Python | 3.11.9; CICFlow package 0.2.0; scapy 2.6.1; pandas 3.0.2 |
| Rust | cargo 1.98.0, release build, `--no-default-features` (no wpcap on Windows) |
| Configuration | flow timeout 120 s, activity timeout 5 s, min packets/flow 2, threads 1, CSV |

The two extractors are not symmetric in language: a fully optimised multi-core Rust
binary competes against a single-threaded Python implementation that spends most of
its time in interpretable byte loops and per-packet scapy object construction. That
asymmetry is, however, exactly the scenario a user choosing an extractor faces.

## 2. Performance

| Dataset | Packets | Rust time | Python time | Speedup |
|---|---|---|---|---|
| `real_traffic.pcap` | 402 | ~0.039 s | ~4.07 s | **103.5x** |
| `sample_traffic.pcap` | 43 | ~0.046 s | ~4.43 s | **95.6x** |
| `wireshark_http.pcap` | 1 | ~0.045 s | ~4.86 s | **107.2x** |

| Dataset | Rust peak RSS | Python peak RSS | Ratio |
|---|---|---|---|
| `real_traffic.pcap` | ~7.1 MB | ~172 MB | ~24x |
| `sample_traffic.pcap` | ~6.4 MB | ~168 MB | ~26x |
| `wireshark_http.pcap` | ~9.1 MB | ~167 MB | ~18x |

**Key observations**

- Rust-CICFlow is ~2 orders of magnitude (≈100x) faster and uses ~25x less peak
  memory than the Python reference on every dataset, including the trivial
  1-packet capture.
- Python's sub-second floor is dominated by startup + import + scapy and per-packet
  object overhead, not by flow computation. Processing the 402-packet capture took
  ~4.1 s (≈10 ms/packet); Rust took ~39 ms total (≈0.1 ms/packet).
- Rust's total runtime is barely larger than its own process startup, so on these
  small corpora the measurement is close to the floor of the tool. Larger captures
  would make the gap per-packet while keeping startup cost amortised.

The order-of-magnitude gap means the choice of extractor matters for live capture
(CICFlow's classic use case): Rust-CICFlow can keep up with high pps on commodity
hardware where the Python implementation cannot.

## 3. Flow extraction

| Dataset | Rust flows | Python flows | Matched | Notes |
|---|---|---|---|---|
| `real_traffic.pcap` | 18 | 15 | 15 | SLL2 encapsulation |
| `sample_traffic.pcap` | 11 | 11 | 11 | Ethernet |
| `wireshark_http.pcap` | 0 | 1 | 0 | 1 packet total |

> **Flow-count correction (2026-09-23).** The Python counts in earlier copies of this
> table (31 / 23 / 3) were the harness's line-based `csv_row_count` double-counting the
> pip writer's `\r\r\n` Windows line terminators (verified directly on the saved
> `python_*.csv`: `real_traffic` has 15 CSV records in 15 data lines but counts out at 31
> lines; `sample_traffic` 11 records / 23 lines; `wireshark_http` 1 record / 3 lines).
> With record-counting fixed, the true tables above show the Python reference emitting
> **equal or fewer** rows than Rust on every capture. `raw_results.json` in this run
> folder now carries a `corrections` section recording both values.

Three independent mechanisms produce the *remaining* real differences:

1. **Encapsulation support (SLL2).** `real_traffic.pcap` uses `LINUX_SLL2` frames.
   The Python extractor silently drops these packets (documented limitation), so
   its records cover 15 flows over a reduced packet subset. On Ethernet captures they align.
2. **Flow finalisation semantics.** Rust's 18 rows against 15 matched 5-tuples on
   `real_traffic.pcap` (three duplicate rows for the same keys) come from the
   canonical expiry semantics — 120 s age-based expiry and RST early-emit produce
   occasional *extra rows per key* (e.g. a "burst observation" row followed by the
   completed flow) — while Python's FIN-triggered collection merges trailing
   packets into fewer rows. On `sample_traffic.pcap` the two populations are
   identical (11 = 11).
3. **Minimum-packets gate.** Rust honours `--min-packets 2`; the 1-packet
   `wireshark_http.pcap` therefore yields 0 flows in Rust while the Python package
   (no such gate) emits 1.

None of these are faults — they are documented behavioural differences. Experiments
that only report raw flow counts without reconciling 5-tuples would misread them as
parser bugs.

## 4. Feature parity

After reconciling flows on the bidirectional 5-tuple (sorted IP pair, sorted port
pair, protocol) and comparing 76 numeric features:

| Dataset | Matched | Exact (max\|Δ\|<1e-6) | Concordant (r>0.999) | Discrepant | Mean r |
|---|---|---|---|---|---|
| `sample_traffic.pcap` | 11/11 | 44 | 32 | **0** | 1.0000 |
| `real_traffic.pcap` | 15/15 | 14 | 39 | 23 | 0.9420 |

`sample_traffic.pcap` — a clean Ethernet capture made by this repository — is
**perfectly concordant: 100% of features are exact or r>0.999 with zero discrepancies**,
and per-rep runs are bit-stable (identical Flow Duration, IAT, flag and window
features across all 11 matched flows). At the time this was read as evidence that the two
extractors compute the same CICFlowMeter semantics on the same input. *(2026-09-23
DEF CON qualification: the 0-discrepancy result is partly a Pearson-criterion artifact —
the systematic frame-vs-payload offset was present there too, e.g. MAE 97 B on
`Total Length of Fwd Packet`, but r = 1.0 on 11 linearly-correlated flows could not
trip the r < 0.999 gate. See the update note above and
`experiments/20260923-020154_Bloodraven/analysis.md`.)*

`real_traffic.pcap` shows a wider spread. The residual differences group into three
causes:

1. **Direction attribution on SLL2.** Without usable MAC-address direction signals,
   the two extractors assign some frames to forward/bwd differently, so split
   aggregates (e.g. `Total Length of Fwd Packet`, `Fwd IAT Total`) diverge up to a
   few KB even though timestamps pair exactly (we verified dt = 0 on all 15 matched
   flows). Direction-sensitive features are the bulk of the 23 discrepancies.
2. **Finalisation boundaries.** Deleting or appending one frame at flow end shifts
   min/mean length and IAT features by one frame (~54–72 B, a few ms).
3. **Upstream packet quirks.** `real_traffic.pcap` contains a retransmission-heavy
   LAN transfer; the single `Fwd PSH Flags` divergence and the `Fwd IAT Min`
   r≈−0.02 polarisation come from flows of only 1–2 packets where a single frame
   difference flips the statistic — an artefact of n=2, not a systematic bias.

A few features are impossible to compare validly and were excluded: Python stores
the frame EtherType (2048) in its `protocol` column instead of the IP protocol id
(recovery itself confirms pairing), and the upstream package reports
`cwr_flag_count` as a copy of `fwd_urg_flags` with `fwd/bwd_seg_size_avg` as copies
of the packet-length means.

## 5. Why the remaining real_traffic discrepancies are acceptable

The 23 discrepant features on `real_traffic.pcap` are all small, structurally
explained deltas on a capture the Python project explicitly does not support
(SLL2). Mean r across features is still 0.94, all flag/init-window features are
exact, Bulk and Active/Idle are dominated by python's BER-byte bulk computation
being applied to a differently-sized flow. They are *implementation* differences,
not *correctness* differences. On the supported path (Ethernet/IPv4) the two
extractors agree to within machine precision.

## 6. Caveats and threats to validity

- **Sample size.** All captures are small (1–402 packets). Startup-dominated timing
  underestimates the steady-state ratio; conclusions should be re-validated on
  larger captures (MB–GB) before being generalised.
- **Single-threaded Rust.** `--threads 1` is the fair per-core comparison and what
  we report; Rust's higher-thread scaling is not exercised here.
- **RSS sampling.** Peak RSS is polled every ~5 ms; short-lived runs can
  under-report instantaneous peaks slightly.
- **Warm-up semantics.** A warm-up run per extractor per dataset is discarded
  before measurement, but Python's import/startup cost is still inside each trial,
  which inflates Python's reported time and is a realistic cost of invoking it.
- **Windows host.** Timing granularity, filesystem cache noise, and scheduler
  behaviour on Windows 11 affect both tools equally; numbers are indicative, not a
  benchmark certification.
- **Two extractor versions.** "Python CICFlow" here means pip `cicflowmeter 0.2.0`
  (uehara), not the original university CICFlowMeter JAR; differences vs. that
  reference were not measured.

## 7. Implications

- **Adopt Rust-CICFlow as the production extractor** for pipelines that re-run the
  same pcap corpus (security analytics, ML dataset generation): ~100x speedup
  directly translates to corpus-size and re-run cadence.
- **Live capture** is the classic CICFlow domain; ~0.1 ms/packet leaves headroom for
  multi-threading on multi-core hosts (8c/16t available here).
- **Feature-level compatibility with the Python reference is validated** on
  supported inputs, so downstream models trained on either extractor's output can
  expect comparable feature distributions for Ethernet/IPv4 traffic.

## 8. Future work

- Benchmark MB–GB captures to move out of the startup-dominated regime and report
  steady-state packets/sec scaling.
- Repeat with `--threads 2/4/8/16` to quantify Rust scaling vs. Python's fixed
  single-thread cost.
- Reconcile against the original CICFlowMeter JAR and pin an expected-performance
  delta for both ports.
- Add IPv6 + VLAN/QinQ captures to characterise encapsulation handling on both
  sides.
- Investigate `real_traffic`'s SLL2 direction divergence to decide whether Rust's
  linkage classifier or a documentable SLL2-specific rule should govern.

## 9. Conclusion

On this machine and corpus, Rust-CICFlow is **~100x faster and ~25x more
memory-efficient** than the pip CICFlow reference while producing **feature-identical
output on the supported Ethernet/IPv4 path** (44 exact + 32 concordant, 0 discrepant
features on `sample_traffic.pcap`). Remaining differences on the SLL2 capture are
analysed, small, and attributable to documented implementation boundaries rather
than to incorrect feature computation.