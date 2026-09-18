# Systems Architecture: Rust-CICFlow — High-Speed Network Flow Telemetry for Next-Generation Cybersecurity Datasets

---

## Overall System Architecture Overview

`cicflowmeter-rust` is designed as a high-throughput, pipelined, four-stage zero-copy network flow telemetry engine. The architecture eliminates dynamic memory allocations on the packet ingestion critical path, bounds per-flow state storage to strictly $\mathcal{O}(1)$ space, and decouples CPU-intensive feature serialization from network wire ingestion.

```
+----------------------------------------------------------------------------------------------------+
|                                    cicflowmeter-rust ARCHITECTURE                                  |
+----------------------------------------------------------------------------------------------------+
|                                                                                                    |
|  [ STAGE 1: INGESTION & ZERO-COPY DECAPSULATION ]                                                  |
|  Raw Wire / PCAP Ingest (AF_PACKET / AF_XDP / Libpcap)                                             |
|        |                                                                                           |
|        v                                                                                           |
|  Zero-Copy Multi-Layer Decapsulator (DLT 1, 113, 276, 0, 108 / VLAN / QinQ / IPv4 / IPv6 / TCP)   |
|        |                                                                                           |
|        +-------------------------------------------------------------------------+                 |
|                                                                                  |                 |
|                                                                                  v                 |
|  [ STAGE 2: LOCK-FREE PARALLEL DISPATCH ]                                 BorrowedPacket<'a>       |
|  Work-Stealing Rayon Worker Pool & Canonical Hash Affinity Dispatcher     (Ephemeral Stack View)   |
|        |                                                                                           |
|        +-----------------------------------+-------------------------------------+                 |
|        | Worker 1                          | Worker 2                            | Worker N        |
|        v                                   v                                     v                 |
|  [ STAGE 3: STATE TRACKING & STREAMING MATH ]                                                      |
|  Core-Pinned AHashMap Flow Shards (Bounded 416-byte Structs)                                       |
|  +-----------------------------------------------------------------------------+                  |
|  | • Deterministic Symmetric TCP Teardown FSM (4-Way FIN & RST Tracking)        |                  |
|  | • Single-Pass Numerically Stable Welford Streaming Moments Accumulators     |                  |
|  | • 2-State Markovian Active/Idle Micro-Burst State Machine                   |                  |
|  | • Streaming Subflow & Bulk Burst Rate Evaluator                             |                  |
|  +-----------------------------------------------------------------------------+                  |
|        |                                                                                           |
|        v                                                                                           |
|  [ STAGE 4: ASYNCHRONOUS LOCK-FREE EGRESS ]                                                        |
|  Bounded Crossbeam Ring Channel (`crossbeam_channel::bounded`)                                     |
|        |                                                                                           |
|        +-------------------------+-------------------------+                                       |
|        v                         v                         v                                       |
|  Canonical CSV Writer     NDJSON Streaming Export    Apache Arrow / NumPy Tensors                  |
+----------------------------------------------------------------------------------------------------+
```

---

## Subsystem 1: Zero-Copy Multi-Layer Protocol Decapsulation

The decapsulation subsystem decodes framed network protocols in a single pass without allocating heap memory, using Rust's pattern matching and lifetime borrowing.

```
+-------------------------------------------------------------------------------+
|                       RAW PACKET BUFFER (&'a [u8] SLICE)                      |
+-------------------------------------------------------------------------------+
                                       |
                                       v
                               [ Inspect Link DLT ]
                               /         |        \
                              /          |         \
               (DLT 1: Eth II)   (DLT 113: SLL)   (DLT 0: BSD Loopback)
                    Offset 14B        Offset 16B       Offset 4B
                              \          |         /
                               \         |        /
                                v        v       v
                     +---------------------------------------+
                     | EtherType == 0x8100 / 0x88A8 / 0x9100?|
                     +---------------------------------------+
                                  /            \
                       (Yes)     /              \ (No)
                                v                v
                     [ Unwrap VLAN/QinQ ]   [ EtherType == 0x0800 / 0x86DD? ]
                     (Offset +4B Loop)           /                      \
                                                / (IPv4)                 \ (IPv6)
                                               v                          v
                                      [ Parse IPv4 Header ]      [ Parse IPv6 Header ]
                                      (Dynamic IHL Slicing)      (40B Fixed Header)
                                               \                          /
                                                \                        /
                                                 v                      v
                                         [ Parse TCP / UDP Transport Layer ]
                                         (Ports, Flags, Window, Payload Slice)
                                                           |
                                                           v
                                            +-------------------------------+
                                            |       BorrowedPacket<'a>      |
                                            | (Ephemeral Stack Descriptor)  |
                                            +-------------------------------+
```

### Protocol Handling Mechanics
1. **Ethernet II (DLT 1)**: Extracts 14-byte MAC framing and checks EtherType.
2. **IEEE 802.1Q & IEEE 802.1ad Nested QinQ**: Executes an inlined `while` loop that recursively peels 4-byte VLAN tags, advancing offsets dynamically without memory copying.
3. **Linux Cooked SLL (DLT 113) & SLL2 (DLT 276)**: Natively parses 16-byte and 20-byte Linux packet socket framing.
4. **BSD Loopback (DLT 0, DLT 108)**: Decodes 4-byte AF family integers with endian-safe conversions.
5. **Dynamic IPv4 IHL**: Extracts Internet Header Length field $(\text{IHL} \ \& \ 0\text{x}0\text{F}) \times 4$, handling variable-length IP option fields ($20 \le \text{IHL} \le 60$ bytes).
6. **IPv6 Support**: Full dual-stack handling for 40-byte fixed headers and 128-bit addresses.
7. **Transport Headers**: Decodes TCP flags, window advertisements, and payload slice boundaries without heap copying.

---

## Subsystem 2: Deterministic Bidirectional TCP Lifecycle State Machine

To eliminate the TCP state-splitting bug identified by Engelen et al. (IEEE SPW 2021), the TCP lifecycle is governed by a fully symmetric Finite State Machine (FSM):

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

### State Transition Rules
1. **INIT $\rightarrow$ ACTIVE**: Triggered by the first observed SYN or data packet. Initializes the canonical 5-tuple key and assigns initiator direction.
2. **ACTIVE $\rightarrow$ FIN_FWD**: Forward endpoint transmits a FIN packet. The flow remains active to ingest trailing backward packets.
3. **ACTIVE $\rightarrow$ FIN_BWD**: Backward endpoint transmits a FIN packet. The flow remains active to ingest trailing forward packets.
4. **FIN_FWD / FIN_BWD $\rightarrow$ TERMINATED**: Reverse endpoint completes the 4-way teardown handshake with its own FIN packet. The completed flow is evicted and flushed.
5. **Immediate RST Termination**: Any valid TCP RST packet unconditionally terminates the flow from any active state.
6. **Inactivity Eviction**: If a flow experiences an inactivity gap exceeding $\tau_{\text{flow}} = 120.0$\,s, it is evicted deterministically.

---

## Subsystem 3: Bounded 416-Byte Flow Struct Memory Layout

Every active flow record is stored in a fixed-size, stack-allocated data structure that occupies exactly **416 bytes**, fitting within 6.5 cache lines (64 bytes each) for cache locality:

```
+-------------------------------------------------------------------------------+
|                      ACTIVE FLOW STRUCT LAYOUT (416 BYTES)                    |
+-------------------------------------------------------------------------------+
| Byte Offset | Size (Bytes) | Field / Subsystem Component                      |
+-------------+--------------+--------------------------------------------------+
| 0x000..0x028| 40 bytes     | Canonical 5-Tuple Key (IPs, Ports, Protocol)     |
| 0x028..0x040| 24 bytes     | Flow Timestamps (t_start, t_last, duration)      |
| 0x040..0x070| 48 bytes     | Forward Packet Length StreamingStats (Welford)   |
| 0x070..0x0A0| 48 bytes     | Backward Packet Length StreamingStats (Welford)  |
| 0x0A0..0x0D0| 48 bytes     | Global Packet Length StreamingStats (Welford)    |
| 0x0D0..0x100| 48 bytes     | Forward IAT StreamingStats (Welford)             |
| 0x100..0x130| 48 bytes     | Backward IAT StreamingStats (Welford)            |
| 0x130..0x160| 48 bytes     | Global Flow IAT StreamingStats (Welford)         |
| 0x160..0x170| 16 bytes     | TCP Flags Bitmask Accumulators (FIN..ECE)        |
| 0x170..0x180| 16 bytes     | TCP Window & Header Byte Accumulators            |
| 0x180..0x190| 16 bytes     | Subflow Window State & Counters                  |
| 0x190..0x198| 8 bytes      | Bulk Burst Directional FSM State                 |
| 0x198..0x1A0| 8 bytes      | TCP Lifecycle State Flags & Padding              |
+-------------------------------------------------------------------------------+
| TOTAL SIZE  | 416 BYTES    | STRICTLY O(1) SPACE GUARANTEE                    |
+-------------------------------------------------------------------------------+
```

---

## Subsystem 4: Lock-Free Work-Stealing Multi-Core Dispatcher

To achieve near-linear parallel scaling across 64 physical cores:

```
[ Raw Network Frames / PCAP Blocks ]
                  |
                  v
       [ Canonical 5-Tuple Hashing ]
       hash = AES_NI_AHash(K_canon) % Num_Workers
                  |
                  +---------------------+---------------------+
                  |                     |                     |
                  v                     v                     v
         [ Rayon Worker 1 ]    [ Rayon Worker 2 ]    [ Rayon Worker N ]
         (Pinned to Core 1)    (Pinned to Core 2)    (Pinned to Core N)
                  |                     |                     |
                  v                     v                     v
         [ Flow Shard 1 ]      [ Flow Shard 2 ]      [ Flow Shard N ]
         (Local AHashMap)      (Local AHashMap)      (Local AHashMap)
                  \                     |                     /
                   \                    |                    /
                    v                   v                   v
              [ Lock-Free Bounded Crossbeam Ring Buffer Channel ]
                                        |
                                        v
                          [ Egress Serialization Thread ]
                          (Canonical CSV / NDJSON / Arrow)
```

1. **Zero Lock Contention**: Because every packet belonging to a bidirectional connection maps to the identical core shard via $\text{AHash}(\mathcal{K}_{\text{canon}}) \pmod W$, packet updates execute with **zero mutex locking**.
2. **Rayon Work-Stealing**: If a single worker experiences traffic burst skew, other idle workers steal tasks from the local queue deque without central synchronization bottlenecks.
3. **Asynchronous Egress**: Flow eviction writes completed records into a non-blocking `crossbeam_channel::bounded` ring buffer, isolating high-speed packet ingestion from disk I/O.

---

## Subsystem 5: Adversarial Hardening & Defense-in-Depth Pipeline

```
[ Ingress Raw Frames ]
        |
        v
+-------------------------------+
|  Stage 1: Frame Guard         | ---> Truncated/Illegal Headers Discarded
+-------------------------------+
        |
        v
+-------------------------------+
|  Stage 2: Table Capacity Guard| ---> |T_active| <= C_table (1,000,000 Flows)
+-------------------------------+
        |
        | If Table Full:
        v
+-------------------------------+
|  Stage 3: Embryonic Ring Flush| ---> Purge O(1) Single-Packet Unacknowledged Flows
+-------------------------------+
        |
        | If Room Available:
        v
+-------------------------------+
|  Stage 4: Active Flow Shard   | ---> Update 416-byte Bounded Flow Record
+-------------------------------+
```

1. **Zero-Copy Frame Guard**: Truncated frames or cyclical VLAN tags are rejected with zero allocation.
2. **Table Capacity Ceiling ($\mathcal{C}_{\text{table}}$)**: Strict bound of $1{,}000{,}000$ active flows prevents RAM exhaustion under multi-gigabit flooding.
3. **$\mathcal{O}(1)$ Embryonic Eviction Ring**: Under volumetric SYN floods, single-packet unacknowledged flows ($N_{\text{fwd}}=1, N_{\text{bwd}}=0$) are purged in $\mathcal{O}(1)$ time from a circular ring buffer, protecting active bidirectional sessions.
4. **AHash Randomization**: 64-bit entropy keys initialized per process prevent hash collision algorithmic complexity attacks.
5. **Wire-Observation TCP Processing**: Retransmitted and out-of-order packets update running moments directly on arrival, eliminating the need to buffer out-of-order segments in RAM.
