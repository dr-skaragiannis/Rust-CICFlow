"""Split a large file into exact-size byte partitions for Zenodo upload.

Zenodo caps single-file uploads at 50 GB; the full DEF CON 26 CTF pcap is
52.92 GB. This creates byte partitions of default 20 GB each, whose
byte-concatenation reproduces the original file bit-for-bit (chunks match
against the parent SHA-256 in SHA256SUMS.txt).

Usage:
    python zenodo_split.py [source] [outdir] [--size-gb 20]
"""
import hashlib, os, sys

DEFAULT_SRC = (r'C:\Users\bloodraven\AppData\Local\Temp\opencode\defcon26'
               r'\defcon26_extracted\DEF CON 26 ctf packet captures.pcap')
DEFAULT_OUT = (r'C:\Users\bloodraven\AppData\Local\Temp\opencode\defcon26'
               r'\zenodo_parts')


def sha256_of(path):
    h = hashlib.sha256()
    with open(path, 'rb') as f:
        for chunk in iter(lambda: f.read(8 * 1024 * 1024), b''):
            h.update(chunk)
    return h.hexdigest()


def main():
    args = sys.argv[1:]
    if args and os.path.exists(args[0]):
        src = args[0]
        rest = args[1:]
    else:
        src = DEFAULT_SRC
        rest = args
    outdir = rest[0] if rest else DEFAULT_OUT
    size_gb = float(rest[1].split('=')[1]) if len(rest) > 1 else 20.0
    chunk = int(size_gb * (1024 ** 3))

    os.makedirs(outdir, exist_ok=True)
    base = os.path.splitext(os.path.basename(src))[0].replace(' ', '_')
    total = os.path.getsize(src)
    nparts = (total + chunk - 1) // chunk
    print(f"splitting {src} ({total/1e9:.2f} GB) into {nparts} parts of {size_gb} GB -> {outdir}")

    manifest_path = os.path.join(outdir, 'SHA256SUMS_parts.txt')
    with open(src, 'rb') as f, open(manifest_path, 'w', encoding='utf-8') as mf:
        for i in range(nparts):
            path = os.path.join(outdir, f"{base}.part{str(i + 1).zfill(2)}of{str(nparts).zfill(2)}")
            h = hashlib.sha256()
            with open(path, 'wb') as w:
                left = min(chunk, total - f.tell())
                copied = 0
                while copied < left:
                    buf = f.read(min(8 * 1024 * 1024, left - copied))
                    if not buf:
                        break
                    w.write(buf)
                    h.update(buf)
                    copied += len(buf)
            digest = h.hexdigest()
            mf.write(f"{digest}  {os.path.basename(path)}\n")
            print(f"  wrote {os.path.basename(path)} ({copied/1e9:.2f} GB) sha256={digest[:16]}...", flush=True)
    print('done. reassemble with:  copy /b part01+part02+... out.pcap   (or cat part* > out.pcap)')
    print('then verify parent SHA-256 (see SHA256SUMS.txt).')
    print('partials manifest:', manifest_path)


if __name__ == '__main__':
    main()
