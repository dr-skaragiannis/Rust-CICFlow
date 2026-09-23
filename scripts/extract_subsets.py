"""Slice 3 representative subsets from the DEF CON 26 CTF pcapng.

Subsets: head (first CHUNK packets), mid (CHUNK packets around ~50% offset),
tail (last CHUNK packets). Raw block-copy preserves packets byte-for-byte.
Two streaming passes: count, then extract selected blocks into RAM buffers.
"""
import sys

def u32(b, endian):
    return int.from_bytes(b, endian)

def main():
    path, outdir = sys.argv[1], sys.argv[2]
    chunk = int(sys.argv[3]) if len(sys.argv) > 3 else 200_000
    fh = open(path, 'rb')
    size = fh.seek(0, 2)
    fh.seek(0)
    bom = fh.read(4)
    if bom != b'\x0a\x0d\x0d\x0a':
        print("not pcapng"); sys.exit(1)
    fh.read(4)
    bom2 = fh.read(4)
    endian = 'little' if bom2 == b'\x4d\x3c\x2b\x1a' else ('big' if bom2 == b'\x1a\x2b\x3c\x4d' else None)
    if endian is None:
        print("bad BOM"); sys.exit(1)
    fh.seek(0)

    def walk():
        pos = 0
        while pos + 8 <= size:
            fh.seek(pos)
            hdr = fh.read(8)
            if len(hdr) < 8:
                break
            btype = u32(hdr[:4], endian)
            blen = u32(hdr[4:8], endian)
            if blen < 12 or blen > 16_000_000:
                break
            yield pos, btype, blen
            pos += blen

    print("pass A: counting packets...", flush=True)
    total_packets = sum(1 for _, btype, _ in walk() if btype in (6, 3))
    print(f"total packets: {total_packets}")

    mid_start = max(0, total_packets // 2 - chunk // 2)
    mid_end = mid_start + chunk
    tail_start = max(0, total_packets - chunk)
    print(f"head [0,{chunk})  mid [{mid_start},{mid_end})  tail [{tail_start},{total_packets})")

    print("pass B: extracting...", flush=True)
    header = b''
    head_sel, mid_sel, tail_sel = [], [], []
    idx = -1
    for pos, btype, blen in walk():
        if btype in (0x0A0D0D0A, 1):
            fh.seek(pos)
            header += fh.read(blen)
        elif btype in (6, 3):
            idx += 1
            fh.seek(pos)
            blob = fh.read(blen)
            if idx < chunk:
                head_sel.append(blob)
            if mid_start <= idx < mid_end:
                mid_sel.append(blob)
            if idx >= tail_start:
                tail_sel.append(blob)
    print(f"selected head={len(head_sel)} mid={len(mid_sel)} tail={len(tail_sel)}")

    open(f"{outdir}/defcon26_head.pcapng", 'wb').write(header + b''.join(head_sel))
    open(f"{outdir}/defcon26_mid.pcapng", 'wb').write(header + b''.join(mid_sel))
    open(f"{outdir}/defcon26_tail.pcapng", 'wb').write(header + b''.join(tail_sel))
    print("done")

if __name__ == '__main__':
    main()