"""Raw pcapng block walker: count packets, report structure. No packet parsing."""
import sys, time

def u32(b, endian):
    return int.from_bytes(b, {'<': 'little', '>': 'big'}[endian])

def main():
    path = sys.argv[1]
    fh = open(path, 'rb')
    size = fh.seek(0, 2)
    fh.seek(0)
    bom = fh.read(4)
    if bom != b'\x0a\x0d\x0d\x0a':
        print("not pcapng"); sys.exit(1)
    fh.read(4)  # block total length
    bom2 = fh.read(4)
    endian = '<' if bom2 == b'\x4d\x3c\x2b\x1a' else ('>' if bom2 == b'\x1a\x2b\x3c\x4d' else None)
    if endian is None:
        print("bad BOM"); sys.exit(1)
    print(f"endianness: {endian}")
    fh.seek(0)
    counts = {}
    packets = 0
    interfaces = set()
    first_pkt_hdr = None
    pos = 0
    nblocks = 0
    t0 = time.time()
    while pos + 8 <= size:
        fh.seek(pos)
        hdr = fh.read(8)
        if len(hdr) < 8:
            break
        btype = u32(hdr[:4], endian)
        blen = u32(hdr[4:8], endian)
        if blen < 12 or blen > 16_000_000:
            print(f"corrupt blen {blen} at {pos}")
            break
        counts[btype] = counts.get(btype, 0) + 1
        nblocks += 1
        if btype == 6:  # EPB
            packets += 1
            ifid = u32(fh.seek(pos + 12) and fh.read(4), endian)
            interfaces.add(ifid)
            if first_pkt_hdr is None:
                first_pkt_hdr = pos
        elif btype == 3:  # SPB
            packets += 1
            if first_pkt_hdr is None:
                first_pkt_hdr = pos
        if nblocks % 200_000 == 0:
            print(f"  ... {nblocks/1e6:.1f}M blocks, {pos/1e9:.2f} GB, {packets/1e6:.1f}M packets, {time.time()-t0:.0f}s", flush=True)
        pos += blen
    fh.close()
    print(f"total size     : {size/1e9:.2f} GB")
    print(f"blocks         : {nblocks}")
    print(f"packets (EPB+SPB): {packets}")
    print(f"interfaces (EPB ifids seen): {sorted(interfaces)}")
    print(f"block type counts: {counts}")
    print(f"first packet block offset: {first_pkt_hdr}")

if __name__ == '__main__':
    main()