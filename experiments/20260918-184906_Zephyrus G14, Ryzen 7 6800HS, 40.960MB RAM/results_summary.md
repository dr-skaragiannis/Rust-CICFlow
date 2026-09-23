# Results summary — `20260918-184906_Zephyrus G14, Ryzen 7 6800HS, 40.960MB RAM`

Host **Zephyrus G14, Ryzen 7 6800HS, 40.960MB RAM** · 2026-09-18 18:49:06 · reps=3

| Dataset | Rust pkts/s | Python pkts/s | Speedup | Rust time | Python time | Time ratio | Rust RSS (MB) | Packets | Features concordant | Features discrepant |
|---|---|---|---|---|---|---|---|---|---|---|
| real_traffic.pcap | 10,224 | 99 | 103.5 | 0.039 | 4.068 | 103.5x | 7.06 | 402 | 39 | 23 |
| sample_traffic.pcap | 927 | 10 | 95.6 | 0.046 | 4.434 | 95.6x | 6.42 | 43 | 32 | 0 |
| wireshark_http.pcap | 22 | 0 | 107.2 | 0.045 | 4.864 | 107.2x | 9.08 | 1 | 0 | 0 |


---

> **Corrections (2026-09-23).** The 'Python flows' values in this report reflect the
> harness's line-based `csv_row_count` double-counting the pip writer's `\r\r\n`
> terminators. True Python CSV record counts: `real_traffic` 15, `sample_traffic` 11,
> `wireshark_http` 1 (vs reported 31/23/3). Python emitted equal or FEWER rows than
> Rust everywhere: Rust 18 vs Python 15 on `real_traffic` (Rust's extra rows are 3
> duplicate 5-tuple rows from canonical 120 s age expiry), 11 vs 11 identical on
> `sample_traffic`, 0 vs 1 on `wireshark_http` (Rust min-packets gate). See the
> flow-count correction note in `Discussion.md` and the `corrections` block in
> `raw_results.json`.
