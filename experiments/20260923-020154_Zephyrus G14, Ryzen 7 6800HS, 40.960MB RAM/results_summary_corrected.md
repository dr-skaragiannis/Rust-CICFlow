# Results summary — `20260923-020154_Zephyrus G14, Ryzen 7 6800HS, 40.960MB RAM` (corrected)

Host **Zephyrus G14, Ryzen 7 6800HS, 40.960MB RAM** · 2026-09-23 02:01:55 · reps=2

> **Note on flow counts.** The auto-generated `results_summary.md` reported
> Python flow counts double-counted (the pip writer's `\r\r\n` terminators
> fooled `csv_row_count`'s line iteration; e.g. head: 77,135 instead of 38,567).
> The `flows_rust`/`flows_python` columns below are the corrected, true CSV
> record counts. Also see §7 in `analysis.md` for the full 49 GB capture run.

| Dataset | Rust pkts/s | Python pkts/s | Speedup | Rust time | Python time | Time ratio | Rust RSS (MB) | Python RSS (MB) | Packets | True Rust flows | True Python flows | Features concordant | Features discrepant | Mean r |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| defcon26_head.pcapng | 188,977 | 1,301 | 145.3 | 1.058 s | 153.7 s | 145.3x | 96.7 | 260.1 | 200000 | 38,818 | 38,567 | 11 | 62 | 0.74 |
| defcon26_mid.pcapng | 287,288 | 947 | 303.4 | 0.696 s | 211.2 s | 303.4x | 33.4 | 1,562.5 | 200000 | 13,047 | 12,879 | 20 | 53 | 0.78 |
| defcon26_tail.pcapng | 303,389 | 1,366 | 222.1 | 0.659 s | 146.4 s | 222.1x | 26.6 | 1,336.6 | 200000 | 9,239 | 6,705 | 13 | 59 | 0.79 |

## Ablation (canonical constants via `pcmeter_driver_aligned.py`, head only)

Run `20260923-023837_Zephyrus G14, Ryzen 7 6800HS, 40.960MB RAM`: discrepant 62 → **57**, concordant 11 → **16**,
same matched-flow population (38,038).

## Full 49 GB capture (Rust only)

156,114,913 packets → 7,798,789 flows · 4,583.4 MB CSV · ≈11.5 min
(≈226k pkts/s single-thread, end-to-end) · peak working set ~7.8 GB at EOF
(offline write-at-end buffering).
