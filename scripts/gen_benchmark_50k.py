"""Generate `benchmark_50k.pcap`: 50,000 interleaved bidirectional TCP packets
across 500 concurrent flows (100 packets per flow).

Matches the documented Workload 1 description (32.4 MB-class trace, 500
bidirectional sessions, payload distribution 64-576 B) for Welford and
hash-collision stress evaluation. Deterministic (seeded RNG).

Usage:
    python scripts/gen_benchmark_50k.py [output.pcap] [--flows N] [--per-flow N]
"""
import argparse
import random
import struct

SERVER_IP = bytes([10, 1, 0, 1])
SERVER_PORT = 80
ETHERTYPE_IPV4 = 0x0800


def checksum(header: bytes) -> int:
    if len(header) % 2:
        header += b'\x00'
    s = 0
    for i in range(0, len(header), 2):
        s += (header[i] << 8) | header[i + 1]
    while s >> 16:
        s = (s & 0xFFFF) + (s >> 16)
    return (~s) & 0xFFFF


def frame_bytes(src_ip, dst_ip, src_port, dst_port, seq, ack, flags, window, ts, payload, ip_id):
    eth = bytes.fromhex('aabbccddeeff00112233445566778899')[:12] + struct.pack('>H', ETHERTYPE_IPV4)
    tcp = struct.pack('>HHIIHHHH', src_port, dst_port, seq, ack, (20 // 4) << 12 | flags, window, 0, 0)
    total_len = 20 + 20 + len(payload)
    ip = struct.pack('>BBHHHBBH', 0x45, 0, total_len, ip_id & 0xFFFF, 0x4000, 64, 6, 0) + src_ip + dst_ip
    c = checksum(ip)
    ip = ip[:10] + struct.pack('>H', c) + ip[12:]
    frame = eth + ip + tcp + payload
    sec = int(ts)
    usec = int((ts - sec) * 1_000_000) % 1_000_000
    return struct.pack('<IIII', sec, usec, len(frame), len(frame)) + frame


def count_pcap(path):
    n = 0
    with open(path, 'rb') as f:
        f.read(24)
        while True:
            h = f.read(16)
            if len(h) < 16:
                break
            cap = struct.unpack('<I', h[8:12])[0]
            f.seek(cap, 1)
            n += 1
    return n


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('output', nargs='?', default='tests/data/benchmark_50k.pcap')
    ap.add_argument('--flows', type=int, default=500)
    ap.add_argument('--per-flow', type=int, default=100)
    args = ap.parse_args()

    random.seed(42)
    base_ts = 1609459200.0
    clients = [(bytes([10, 0, 101 + fi // 250, 1 + fi % 250]), 40000 + fi) for fi in range(args.flows)]

    with open(args.output, 'wb') as f:
        f.write(struct.pack('<IHHiIII', 0xa1b2c3d4, 2, 4, 0, 0, 262144, 1))
        ts = base_ts
        for pkt_i in range(args.per_flow):
            for fi in range(args.flows):
                cip, cport = clients[fi]
                payload_len = random.choice([0, 0, 64, 128, 256, 384, 512, 576])
                to_forward = random.random() > 0.5
                ts += 0.00005 * (1 + (fi % 11))
                payload = b'x' * payload_len
                ip_id = fi * 1000 + pkt_i
                if pkt_i == 0:
                    flags_fwd, flags_bwd = 0x02, 0x02 | 0x08
                else:
                    base = 0x08 if payload_len else 0x00
                    flags_fwd = flags_bwd = base | 0x10
                seq_f = 1000 + pkt_i * payload_len
                ack_f = 2000 + pkt_i
                seq_b = 1000 + pkt_i * payload_len
                ack_b = seq_f + payload_len + (1 if flags_fwd - (flags_fwd & ~0x02) else 0)
                if to_forward:
                    pkt = frame_bytes(cip, SERVER_IP, cport, SERVER_PORT, seq_f, ack_b,
                                      flags_fwd, 65535, ts, payload, ip_id)
                else:
                    pkt = frame_bytes(SERVER_IP, cip, SERVER_PORT, cport, seq_b, seq_f,
                                      flags_bwd, 64240 if pkt_i == 1 else 65535, ts, payload, ip_id)
                f.write(pkt)
    n = count_pcap(args.output)
    import os
    print(f'wrote {args.output}: {n} packets (target {args.flows * args.per_flow}), '
          f'{os.path.getsize(args.output)/1e6:.1f} MB')


if __name__ == '__main__':
    main()
