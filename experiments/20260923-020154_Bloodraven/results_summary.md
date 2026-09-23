# Results summary — `20260923-020154_Bloodraven`

Host **Bloodraven** · 2026-09-23 02:01:55 · reps=2

| Dataset | Rust pkts/s | Python pkts/s | Speedup | Rust time | Python time | Time ratio | Rust RSS (MB) | Packets | Features concordant | Features discrepant |
|---|---|---|---|---|---|---|---|---|---|---|
| defcon26_head.pcapng | 188,977 | 1,301 | 145.3 | 1.058 | 153.729 | 145.3x | 96.69 | 200000 | 11 | 62 |
| defcon26_mid.pcapng | 287,288 | 947 | 303.4 | 0.696 | 211.230 | 303.4x | 33.40 | 200000 | 20 | 53 |
| defcon26_tail.pcapng | 303,389 | 1,366 | 222.1 | 0.659 | 146.392 | 222.1x | 26.64 | 200000 | 13 | 59 |


---

> **Corrections (2026-09-23).** Python flow counts in this report are line-based and
> double-count the pip writer `\r\r\n` terminators (77,135/25,759/13,411). True CSV
> record counts: 38,567 / 12,879 / 6,705 vs Rust 38,818 / 13,047 / 9,239 — equal or
> fewer Python rows everywhere except the Rust-skewed tail (+37.8%). See corrected
> `results_summary_corrected.md` and `analysis.md` section 3.
