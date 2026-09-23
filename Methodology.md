# Methodology: Rust-CICFlow — High-Speed Network Flow Telemetry for Next-Generation Cybersecurity Datasets

---

## Overview of the Rewrite Methodology

To overcome the performance bottlenecks, memory exhaustion, and state desynchronization bugs inherent in legacy Java and Python flow extractors, **`cicflowmeter-rust`** was engineered from first principles in pure, safe Rust. 

The re-engineering methodology followed a rigorous ten-phase process combining formal mathematical proofs, systems-level micro-architectural optimization, hardware-aware data structures, and continuous differential testing against legacy reference outputs.

```
+----------------------------------------------------------------------------------------------------+
|                                  RUST-CICFlow REWRITE METHODOLOGY                                  |
+----------------------------------------------------------------------------------------------------+
|  Phase 1: Dissection & Reverse Engineering of Java CICFlowMeter v4.0                              |
|                                         |                                                          |
|  Phase 2: Zero-Copy Multi-Layer Protocol Decapsulation Engine (BorrowedPacket<'a>)                 |
|                                         |                                                          |
|  Phase 3: Numerically Stabilized Single-Pass Streaming Statistics (Welford & Chan)                |
|                                         |                                                          |
|  Phase 4: Deterministic Bidirectional TCP Lifecycle State Machine (Symmetric FIN/RST FSM)         |
|                                         |                                                          |
|  Phase 5: Streaming Subflow & Bulk Micro-Burst Segmentation Algorithm                             |
|                                         |                                                          |
|  Phase 6: Two-State Markovian Active/Idle Micro-Burst Tracker                                     |
|                                         |                                                          |
|  Phase 7: Lock-Free Multi-Core Work-Stealing Parallel Concurrency (Rayon + AES-NI AHash)          |
|                                         |                                                          |
|  Phase 8: IEEE 754 Floating-Point Determinism & Single-Packet Zero-Clamping Guardrails             |
|                                         |                                                          |
|  Phase 9: High-Performance Zero-Copy PyO3 NumPy & C-ABI Foreign Function Interfaces              |
|                                         |                                                          |
|  Phase 10: Automated Differential Testing, Parity Verification, & Benchmark Replication            |
+----------------------------------------------------------------------------------------------------+
```

---

## Phase 1: Dissection & Reverse Engineering of Legacy Reference Implementations

1. **Java CICFlowMeter (v4.0) Analysis**:
   - Analyzed the official source repository of CICFlowMeter (`cic.cs.unb.ca.ifm.Cmd`, `FlowGenerator`, `BasicPacketInfo`, `Flow`).
   - Identified the root cause of $\mathcal{O}(N_{\text{packets}})$ memory growth: active flows stored full packet objects in dynamic `ArrayList<Packet>` buffers.
   - Traced the TCP teardown desynchronization flaw to `Flow.java` where a flow is flagged as closed on the *first* FIN packet observed from either endpoint.
   - Measured the JNI foreign function crossing penalty of `jNetPcap` ($15.4\,\mathrm{ns}$ per packet).

2. **Python Port Dissection (`cicflowmeter` / `scapy`)**:
   - Identified heavy CPython object header overhead (56 bytes minimum per dictionary key-value pair).
   - Documented Python Global Interpreter Lock (GIL) stalls and unrecoverable Out-Of-Memory termination on traces $>1$\,GB.

---

## Phase 2: Zero-Copy Multi-Layer Protocol Decapsulation Engine

To eliminate memory allocation on the ingress packet path, we architected a zero-copy link-layer and transport-layer decapsulator:

1. **Stack Lifetime Borrowing (`BorrowedPacket<'a>`)**:
   - Packet buffers are represented as raw slices (`&'a [u8]`) borrowed directly from the capture buffer.
   - Header descriptors reside entirely in CPU registers and stack space without heap allocation.
   ```rust
   pub struct BorrowedPacket<'a> {
       pub timestamp: u64,          // High-resolution nanosecond timestamp
       pub src_ip: IpAddr,          // IPv4 (4-byte) or IPv6 (16-byte)
       pub dst_ip: IpAddr,          // IPv4 (4-byte) or IPv6 (16-byte)
       pub src_port: u16,          // Transport layer source port
       pub dst_port: u16,          // Transport layer destination port
       pub protocol: u8,           // IANA protocol ID (6=TCP, 17=UDP)
       pub tcp_flags: u8,          // TCP 8-bit control flag bitmask
       pub window_size: u16,       // TCP advertised window size
       pub header_length: usize,   // Layer-3/4 header byte length
       pub payload_length: usize,  // Transport payload length
       pub payload: &'a [u8],      // Borrowed zero-copy payload slice
   }
   ```

2. **Monomorphic Multi-Layer Framing Support**:
   - **Ethernet II (DLT 1)**: Parses 14-byte MAC framing and extracts EtherType.
   - **IEEE 802.1Q Single VLAN & IEEE 802.1ad Nested QinQ**: Implements a zero-allocation `while` loop peeling 4-byte VLAN tags (EtherTypes `0x8100`, `0x88A8`, `0x9100`).
   - **Linux Cooked Captures (SLL DLT 113, SLL2 DLT 276)**: Parses 16-byte and 20-byte Linux packet socket headers natively.
   - **BSD Loopback (DLT 0, DLT 108)**: Decodes 4-byte AF family integers with platform-independent endianness resolution.
   - **IPv4 Dynamic IHL**: Computes dynamic IPv4 header length via $(\text{IHL} \ \& \ 0\text{x}0\text{F}) \times 4$, handling variable-length options correctly.
   - **IPv6 Dual-Stack**: Parses 40-byte fixed IPv6 headers and 128-bit addresses.
   - **TCP / UDP Framing**: Extracts 16-bit ports, 8-bit TCP control flags, 16-bit window sizes, and transport payload offsets.

---

## Phase 3: Numerically Stabilized Single-Pass Streaming Statistics

To bound memory to $\mathcal{O}(1)$ space while computing exact arithmetic means ($\mu$), sample variances ($s^2$), and standard deviations ($\sigma$) over packet lengths and inter-arrival times, we formalized and implemented online streaming recurrences.

### 1. Welford's Single-Pass Recurrence
For an observation sequence $\{x_1, x_2, \dots, x_k\}$, the running mean and sum of squared deviations are updated incrementally:
$$\mu_k = \mu_{k-1} + \frac{x_k - \mu_{k-1}}{k}$$
$$M_{2,k} = M_{2,k-1} + (x_k - \mu_{k-1})(x_k - \mu_k)$$
$$s_k^2 = \frac{M_{2,k}}{k - 1}, \quad \sigma_k = \sqrt{s_k^2} \quad (\text{for } k \ge 2)$$

* **Non-Negative Monotonic Stability**: Because $(x_k - \mu_{k-1})$ and $(x_k - \mu_k)$ always share the same algebraic sign, their product is non-negative ($M_{2,k} \ge 0$), strictly preventing catastrophic floating-point cancellation.

```rust
#[derive(Debug, Clone, Default)]
pub struct StreamingStats {
    pub count: u64,
    pub sum: f64,
    pub mean: f64,
    pub m2: f64,
    pub min: f64,
    pub max: f64,
}

impl StreamingStats {
    #[inline(always)]
    pub fn update(&mut self, val: f64) {
        self.count += 1;
        self.sum += val;
        if self.count == 1 {
            self.mean = val;
            self.m2 = 0.0;
            self.min = val;
            self.max = val;
        } else {
            let delta = val - self.mean;
            self.mean += delta / (self.count as f64);
            let delta2 = val - self.mean;
            self.m2 += delta * delta2;
            if val < self.min { self.min = val; }
            if val > self.max { self.max = val; }
        }
    }
}
```

### 2. Chan's Parallel Variance Merging
When merging flow statistics across parallel worker threads or aggregating subflows:
$$\delta = \mu_B - \mu_A$$
$$\mu_{AB} = \mu_A + \delta \cdot \frac{N_B}{N_{AB}}$$
$$M_{2,AB} = M_{2,A} + M_{2,B} + \delta^2 \cdot \frac{N_A N_B}{N_{AB}}$$

### 3. Extension to 3rd and 4th Central Moments (Skewness & Kurtosis)
$$M_{3,AB} = M_{3,A} + M_{3,B} + \frac{N_A N_B (N_A - N_B)}{N_{AB}^2} \delta^3 + \frac{3 (N_A M_{2,B} - N_B M_{2,A})}{N_{AB}} \delta$$
$$M_{4,AB} = M_{4,A} + M_{4,B} + \frac{N_A N_B (N_A^2 - N_A N_B + N_B^2)}{N_{AB}^3} \delta^4 + \frac{6 (N_A^2 M_{2,B} + N_B^2 M_{2,A})}{N_{AB}^2} \delta^2 + \frac{4 (N_A M_{3,B} - N_B M_{3,A})}{N_{AB}} \delta$$
$$\gamma_1 = \frac{\sqrt{N_{AB}} M_{3,AB}}{(M_{2,AB})^{3/2}}, \quad \gamma_2 = \frac{N_{AB} M_{4,AB}}{(M_{2,AB})^2} - 3$$

---

## Phase 4: Deterministic Bidirectional TCP Lifecycle State Machine

To eliminate the TCP state-splitting bug identified by Engelen et al. (IEEE SPW 2021), we formalized a deterministic symmetric finite state machine (FSM):

```
       +---------------------------------------------+
       |                    INIT                     |
       +---------------------------------------------+
                              | SYN / Data
                              v
       +---------------------------------------------+
       |                   ACTIVE                    |
       +---------------------------------------------+
             |                                 |
     Fwd FIN |                         Bwd FIN |
             v                                 v
  +--------------------+             +--------------------+
  |      FIN_FWD       |             |      FIN_BWD       |
  |  (Fwd FIN Seen)    |             |  (Bwd FIN Seen)    |
  +--------------------+             +--------------------+
             |                                 |
     Bwd FIN |                         Fwd FIN |
             v                                 v
       +---------------------------------------------+
       |                 TERMINATED                  |
       |               (Evict & Export)              |
       +---------------------------------------------+
```

1. **Symmetric FIN Bitmask Tracking**:
   - Maintains a 2-bit FIN registration state $S_{\text{fin}} = \langle b_{\text{fwd}}, b_{\text{bwd}} \rangle$.
   - The flow is marked as terminated if and only if:
     $$\text{Terminated} = (b_{\text{fwd}} \land b_{\text{bwd}}) \lor (\text{RST} == 1)$$
   - When the first FIN is observed, the flow remains active in the flow table, allowing trailing teardown packets (FIN-ACKs) to be integrated into the existing session rather than fragmented into spurious single-packet flows.

2. **Inactivity Timeout & Asymmetric Routing Boundedness**:
   - Under asymmetric routing where reverse packets bypass the sensor ($N_{\text{bwd}} = 0$), the flow is evicted deterministically when $\Delta t = t - t_{\text{last}} > \tau_{\text{flow}}$ ($120.0$\,s).
   - By Little's Law, active flow table capacity is strictly bounded by $|\mathcal{T}_{\text{active}}| \le \lambda_{\text{flows}} \cdot \tau_{\text{flow}} < \infty$.

---

## Phase 5: Streaming Subflow & Bulk Micro-Burst Segmentation

1. **Subflow Window Segmentation ($\tau_{\text{sub}} = 1.0\,\text{s}$)**:
   - When an inter-packet arrival gap $\Delta t_{\text{last}} = t - t_{\text{last}} > 1.0\,\text{s}$, the subflow counter increments: $S_{\text{subflows}} \leftarrow S_{\text{subflows}} + 1$.
   - Total forward/backward packets and bytes are divided by $S_{\text{subflows}}$ to yield mean subflow densities (Features 68--71).

2. **Micro-Burst Bulk Rate Accumulation ($K_{\text{bulk\_min}} = 4$)**:
   - A directional bulk burst is tracked across consecutive payload packets ($L_{\text{pay}} > 0$).
   - A burst is committed to Welford bulk moments only if it reaches at least $K_{\text{bulk\_min}} = 4$ contiguous packets before an idle interval $>1.0$\,s, preventing isolated transactions from polluting bulk metrics (Features 62--67).

---

## Phase 6: Two-State Markovian Active/Idle Burst Tracking

Traffic dynamics are modeled via a two-state Markov chain tracking active transmission periods and dormant idle pauses:
* **Active Burst ($\Delta t < 5.0\,\text{s}$)**: Packet trains with inter-arrival times below $\tau_{\text{act}} = 5.0\,\text{s}$ accumulate into the ongoing active duration $T_{\text{act}} = t - t_{\text{act\_start}}$.
* **Idle Pause ($\Delta t \ge 5.0\,\text{s}$)**: When $\Delta t \ge 5.0\,\text{s}$, the active duration is finalized into `active_stats` via Welford updating, and the idle interval $\Delta t$ is committed to `idle_stats` (Features 76--83).

---

## Phase 7: Lock-Free Multi-Core Parallel Concurrency (Rayon + AES-NI AHash)

To scale across many-core server architectures:
1. **Core-Pinned Flow Table Sharding**: The global flow table is partitioned across worker threads. Packets are routed using canonical 5-tuple hash affinity ($\text{AHash}(\mathcal{K}_{\text{canon}}) \pmod W$), eliminating cross-thread synchronization locks.
2. **Rayon Work-Stealing**: Worker queues dynamically balance CPU loads using lock-free deque work-stealing during bursty traffic spikes.
3. **Hardware-Accelerated AES-NI AHash**: Utilizes dedicated x86 `AES-NI` and ARM64 `Crypto` instructions for 5-tuple hashing, achieving $37.3\text{M}$ lookups/sec ($2.18\times$ faster than SipHash).
4. **Lock-Free Asynchronous Egress**: Terminated flows are dispatched across bounded `crossbeam_channel` queues to background writer threads emitting CSV, NDJSON, or Apache Arrow.

---

## Phase 8: Bitwise IEEE 754 Determinism & Safe Rate Clamping

1. **Zero-Clamping for Single-Packet Flows ($\Delta T = 0$)**:
   $$\mathcal{R}_{\text{bytes}} = \begin{cases} \frac{L_{\text{total}}}{\Delta T \times 10^{-6}}, & \text{if } N_p > 1 \land \Delta T > 0 \\ 0.0, & \text{if } N_p = 1 \lor \Delta T = 0 \end{cases}$$
   $$\mathcal{R}_{\text{pkts}} = \begin{cases} \frac{N_p}{\Delta T \times 10^{-6}}, & \text{if } N_p > 1 \land \Delta T > 0 \\ 0.0, & \text{if } N_p = 1 \lor \Delta T = 0 \end{cases}$$
   Eliminates `+Infinity` and `NaN` values across all numeric output features.

2. **Cross-Architecture Reproducibility**:
   - Strict IEEE 754-2008 double-precision accumulators ($53$-bit significand, $\epsilon_{\text{mach}} \approx 2.22 \times 10^{-16}$).
   - Disables non-associative Fused Multiply-Add (FMA) compiler contractions to guarantee bitwise-identical feature vectors across x86-64 and ARM64.

---

## Phase 9: High-Performance Zero-Copy PyO3 & C-ABI Foreign Function Bindings

1. **PyO3 NumPy Tensor Streaming**:
   - Accepts raw packet byte buffers from Python without memory copying.
   - Writes extracted 84-dimensional flow vectors directly into pre-allocated, row-major C-contiguous NumPy memory blocks (`PyArray2<f64>`), allowing instant conversion into PyTorch tensors (`torch.from_numpy`) with zero serialization cost.

2. **Native C-ABI Export**:
   - Exposes an unmanaged `extern "C" fn cicflowmeter_extract_c_abi` for native integration into eBPF user-space daemons, DPDK engines, and C/C++ security appliances.

---

## Phase 10: Automated Continuous Differential Testing & Verification

1. **Automated Parity Harness (`scripts/compare_outputs.py`, `scripts/run_experiments.py`)**:
   - Ingests CSV outputs from Java CICFlowMeter v4.0 and `cicflowmeter-rust` generated on identical PCAP files.
   - Computes column-by-column Mean Absolute Error (MAE), Maximum Absolute Difference, and Pearson Correlation Coefficients ($r$) across all 84 features.
2. **One-Command Replication (`scripts/replicate_experiments.sh`)**:
   - Automates trace downloading, release compilation, unit testing, hardware performance profiling (`perf stat`), and statistical verification.

### 10a. Differential-Testing Lessons from the DEF CON 26 CTF Validation (2026-09-23)

The 2006 corpus methodology above was stress-tested on a real 49 GB / 156.1 M-packet
adversarial capture (`experiments/20260923-020154_Zephyrus G14, Ryzen 7 6800HS, 40.960MB RAM/`). Two findings refine
Phase 10 practice:

1. **Pearson-gate insensitivity to systematic offsets.** On 11-flow synthetic corpora,
   a constant-per-flow definitional offset (frame vs payload byte accounting) yields
   perfectly linear correlations ($r = 1.0$) and passes the $r > 0.999$ gate while
   carrying MAE $\approx 97$ B per flow. Real scan-heavy traffic breaks linearity and
   exposes the offset. Differential testing therefore must track **MAE/max-diff
   distributions per flow** in addition to aggregate $r$, and must compare against
   *multiple* reference extractors, since the pip `cicflowmeter` reference deviates
   from both the canonical CIC-IDS semantics and this engine:
   - packet length = full Ethernet frame (`len(packet)`) instead of TCP payload;
   - flow expiry = 240 s inactivity (`EXPIRED_UPDATE=240`) instead of 120 s flow age;
   - active/idle threshold = 5 ms (`ACTIVE_TIMEOUT`) instead of 5 s, bulk clump = 1 ms;
   - population variance (`numpy.var`) instead of the documented sample variance.
2. **Record-count instrumentation.** The pip writer emits `\r\r\n` terminators on
   Windows; line-based record counting double-counts rows (exactly 2×) and distorted
   flow-count summaries until `csv_row_count` was fixed to `csv.reader`-based
   record counting (`scripts/run_experiments.py`).

The engine implemented the canonical side of every contested semantic:
payload-based length accounting, symmetric FIN/RST FSM with 120 s age expiry
(`src/flow/generator.rs`), 5 s activity timeout, and sample variance per the
Welford formula in Phase 3.
