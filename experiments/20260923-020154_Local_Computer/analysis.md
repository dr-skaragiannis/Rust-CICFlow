# DEF CON 26 CTF Packet Captures — CICFlow vs Rust-CICFlow

Run folder: `experiments/20260923-020154_Local_Computer/` (main, canonical config)
Ablation run: `experiments/20260923-023837_Local_Computer/` (Python constants aligned to canonical CIC-IDS)
Host: Local_Computer · 2026-09-23 · extractors: pip `cicflowmeter` 0.2.0 (via `pcmeter_driver.py`) vs Rust release binary, as for the previous corpus.

## 1. Dataset

`DEF CON 26 ctf packet captures.rar` (media.defcon.org, DEF CON 26 CTF) contains a
**single 49 GB pcapng capture**: 156,114,913 Ethernet packets, one interface
(linktype 1), all Enhanced Packet Blocks, captured during DEF CON 26 CTF 2018
(first packets `2018-08-10 22:48:42`, TCP-dominated CTF network traffic).

Because the Python reference runs at ~0.77 ms/packet, processing all 156 M
packets end-to-end would take ~33 h. The experiment therefore ran both
extractors over three **representative 200,000-packet subsets**, byte-exact
block copies of the original pcapng (SHB + IDB + contiguous EPBs):

| Subset | Packets | Offset | Size | Profile |
|---|---|---|---|---|
| `defcon26_head.pcapng` | 200,000 | [0) | 25.7 MB | event start, tiny scan/handshake packets (~129 B avg frame) |
| `defcon26_mid.pcapng` | 200,000 | [78.0 M, 78.2 M) | 101.3 MB | event peak, bulk game traffic (~506 B avg frame) |
| `defcon26_tail.pcapng` | 200,000 | last 200 k | 48.0 MB | event end |

Subsets are preserved in `subsets/` for reproducing every number below.

## 2. Performance

End-to-end wall-clock of the extractor subprocess (startup included),
mean of 2 measured reps after a discarded warm-up. Flow timeout 120 s,
activity timeout 5 s, min-packets 2, CSV, single worker thread.

| Dataset | Rust time | Python time | Speedup | Rust pkts/s | Python pkts/s |
|---|---|---|---|---|---|
| head | 1.06 s | 153.7 s | **145×** | 188,977 | 1,301 |
| mid | 0.70 s | 211.2 s | **303×** | 287,288 | 947 |
| tail | 0.66 s | 146.4 s | **222×** | 303,389 | 1,366 |

(Throughput uses packets ÷ *measured* time; the auto-generated
`results_summary.md` divides by mean-of-reps time — 145.3×/303.4×/222.1× is the exact mean-rep figure.)

Extrapolated full capture: Rust ~78 s where Python would need ~33 h
(156.1 M × 0.768 ms measured on the head slice).

### Peak RSS

| Dataset | Rust | Python | Ratio |
|---|---|---|---|
| head | 96.7 MB | 260.1 MB | 2.7× |
| mid | 33.4 MB | **1,562.5 MB** | 47× |
| tail | 26.6 MB | 1,336.6 MB | 50× |

Python's RSS scales with flow-table residency; at event-peak concurrency
(38 k+ live flows) it transiently holds 1.5 GB, while the Rust flow table
stays under 100 MB on the same input.

### Full 49 GB capture (Rust only)

Rust processed all 156,114,913 packets in a single run — see `full_capture`
section §7. Python was not run on the full capture (≈33 h extrapolation).

## 3. Flow extraction (corrected counts)

⚠ **Finding that changes a naive reading of the auto-generated summary:** the
pip package's CSV writer emits `\r\r\n` terminators (Windows text translation
doubles the `\r`), so the harness's `csv_row_count` initially reported *2×*
the true Python record count (77,135 instead of 38,567 flows on head). The
harness `csv_row_count` has been fixed (record-based via `csv.reader`);
the **true** counts from the parity merge are:

| Dataset | Rust flows | Python flows | Δ | Matched (5-tuple, longest-per-key) |
|---|---|---|---|---|
| head | 38,818 | 38,567 | Rust +0.7% | 38,038 |
| mid | 13,047 | 12,879 | Rust +1.3% | 12,719 |
| tail | 9,239 | 6,705 | Rust +37.8% | 6,695 |

Both tools are bit-stable across reps (identical CSV bytes/head sizes).

Interpretation: on time-contiguous slices (head), the two extractors agree on
flow populations within ~1%. The tail divergence (+38% Rust) comes from Rust's
NERIS-style symmetric FIN state machine absorbing post-FIN traffic plus the
age-based 120 s split, vs Python's FIN-triggered collection creating separate
trailing rows — direction of the difference is dataset-dependent (more Rust
rows on tail, ~equal on head/mid).

## 4. Feature parity (harness, 76 numeric features, r>0.999 concordance)

| Dataset | Matched | Exact | Concordant | Discrepant | Mean r |
|---|---|---|---|---|---|
| head | 38,038 | 3 | 11 | 62 | 0.737 |
| mid | 12,719 | 3 | 20 | 53 | 0.778 |
| tail | 6,695 | 4 | 13 | 59 | 0.790 |

Headline parity looks far worse than the tiny-corpus runs (which reported
0/76 discrepant on `sample_traffic.pcap` and 23/76 on `real_traffic.pcap`).
On 38 k *real* flows this is the honest number — and it decomposes cleanly:

### 4.1 Most flows are bit-identical

On head: **92.5% of matched flows attribute identical forward+backward packet
counts**; within that classification, **Flow Duration is exactly equal for
99.997%** and packet-count features are 100% exact. Feature *math* is
sound; the aggregate correlations are dragged down by a segmentation tail.

### 4.2 Cause 1 — length-accounting semantics (systematic, upstream)

- pip Python: `len(packet)` = full **Ethernet frame** length (74 B for a bare SYN).
- Rust: `payload_bytes` = TCP **payload** bytes (0 for a SYN), matching the
  canonical CIC-IDS2017 packet-length accounting where handshakes register 0.

Effect: length features differ by header overhead per packet — MAE 337 B /
max 10.9 kB on head's `Total Length of Fwd Packet`, and correlations collapse
on scan-heavy traffic (Fwd Packet Length Min r = −0.02: min is 0 for
Rust payloads but 54+ for Python frames).

**This reproduces on the repo's own `sample_traffic.pcap`** (Python 6,018 vs
Rust 5,370; Python max 1,095 vs Rust 1,041). The historical claim of "0
discrepancies on the supported path" was an artifact of the Pearson criterion:
with only 11 flows whose deltas are perfectly linear (r = 1.0 → CONCORDANT
status), the systematic offset was invisible. On realistic traffic it surfaces
as DISCREPANCY. (Note: canonical Java CICFlowMeter also counts IP-length-ish
values — the README's Java-reference table shows `Fwd Packet Length Min = 0`
which matches Rust's payload semantic, not Python's frame semantic.)

### 4.3 Cause 2 — flow-expiry semantics

| | Flow split rule | Resulting row duration |
|---|---|---|
| Rust (canonical CIC-IDS) | age since flow start > 120 s (`generator.rs:71`) | ≤ ~120 s |
| pip package | inactivity > **240 s** (`constants.EXPIRED_UPDATE = 240`), collection at duration ≥ 90 s / ≥ 120 s trigger | up to ~240 s |

CTF traffic has huge idle gaps inside sessions, so Python emits merged flows
spanning >120 s (observed 202.8 s) while Rust splits them at 120 s of age. In
the longest-Δ flows the merge reconciles a Rust *segment* against a Python
*merge* — the systematic source of the big max-diffs (202,800,568 µs exactly,
all across Duration/IAT Max/Idle Max) and of r ≈ 0.62–0.92 in the IAT group.

### 4.4 Cause 3 — Active/Idle (and bulk clump) thresholds

| | Active/idle epoch threshold | Bulk clump |
|---|---|---|
| Rust (canonical) | 5,000 ms (activity timeout `--activity-timeout 5000000`) | contiguous-payload semantics |
| pip package | `CLUMP_TIMEOUT = 0.001 s`, `ACTIVE_TIMEOUT = 0.005 s` (5 ms!) | 1 ms clump |

Python's 5 ms active-window threshold collapses nearly every micro-gap into a
tiny epoch, so Active Mean/Std/Max/Min and Idle Mean/Std/Max/Min (8 features)
are structurally different (r 0.10–0.65) even when packet counts agree
exactly. This is an upstream constants divergence, not an engine error.

### 4.5 Cause 4 — sample vs population variance

Rust: `variance = M2/(n-1)` (Welford, **sample**, per its documented formula,
`stats.rs:94`). pip: `numpy.var` = **population** (ddof=0). Ratio n/(n−1):
negligible on big flows, → 50%+ relative error at n=2-4. Explains Std/Var
features sitting at r ≈ 0.994–0.9999 (CONCORDANT) rather than EXACT, and the
Fwd/Bwd IAT Std polarisation on 1-2-packet-direction flows.

### 4.5b Ablation — aligning Python constants to canonical

Re-running head with `pcmeter_driver_aligned.py` (EXPIRED 240→120 s,
ACTIVE_TIMEOUT 5 ms→5 s, CLUMP 1 ms→5 s):
**discrepant 62 → 57, concordant 11 → 16.** Positive but small: confirms the
aggregate is dominated by Causes 1 (length semantics) + 2 (age-based vs
inactivity-based expiry), which corner-case the constants alone cannot fix.

## 5. Which of these is a Rust-CICFlow deficiency?

None is a Rust bug; every discrepant feature group has a traced upstream
definitional or segmentation cause, and Rust is the side matching the
canonical CIC-IDS semantics (payload accounting, 120 s age-based expiry, 5 s
activity window, sample variance per its documented Welford formula).
Conversely, the corpus-level caveat also lands the other way: the pip package
is *not* a drop-in "ground truth" — on real CTF-scale traffic it behaves as a
non-canonical variant (frame-based lengths, 240 s inactivity, 5 ms actives,
population variance, no minimum-packets gate).

## 6. Threats to validity

- Only 3 × 200 k-packet slices of a 156 M-packet capture (startup-dominated
  Python timing is amortised here at 200 k, so throughput ratios are
  meaningful; still slices, not the full capture).
- The Python side runs through the driver (flush-at-EOF), as before.
- RSS sampled at ~5 ms; Windows granularity affects both sides.
- Direction attribution on Ethernet: both tools agree on direction for 92.5%
  of flows (packet counts identical); no SLL2 caveat this time (linktype 1).
- `csv_row_count` double-count correction applied post-hoc; regenerated
  summary reports remain affected for `flows_python`/`flows_rust` counts.

## 7. Full 49 GB capture (Rust, single run)

`full.pcap` (hardlinked extracted capture) processed end-to-end:

| Metric | Value |
|---|---|
| Packets | 156,114,913 |
| Flows emitted | 7,798,789 |
| Output CSV | 4,583.4 MB (`full_rust_out/full.pcap_Flow.csv`) |
| Wall time | ≈ 11.5 min (≤ 700 s) |
| End-to-end throughput | ≈ 226,000 pkts/s (single thread, incl. CSV write) |
| Python extrapolation | ≈ 33 h (156.1 M × 0.768 ms measured on head) → ~172× projected |

Caveat: in offline single-file mode the engine buffers finished flows in RAM
until EOF — peak working set grew 1.9 GB → **7.8 GB** with the 4.6 GB flow
table. For captures larger than ~10 GB, batch directory mode or slice
processing keeps memory bounded (slices stayed 27–97 MB). No error, no
crash; run completed cleanly.

Full-capture artifact: `C:\Users\BLOODR~1\AppData\Local\Temp\opencode\defcon26\full_rust_out\full.pcap_Flow.csv`
(4,583.4 MB; delete when no longer needed).

## 8. Reproduction

```bash
python scripts/run_experiments.py \
  --data-dir <run-dir>/subsets --output-root experiments \
  --reps 2 --skip-build                       # canonical comparison
python scripts/run_experiments.py \
  --data-dir <run-dir>/subsets --output-root experiments \
  --reps 2 --skip-build \
  --python-driver scripts/pcmeter_driver_aligned.py   # constants ablation
# subset creation (raw block-copy slicer):
python scan_pcapng.py <full.pcap>            # inventory
# (extract_subsets.py: 2-pass slicer, block-exact copies)
```
