#!/usr/bin/env python3
"""
CICFlow vs Rust-CICFlow experiment harness.

Automatically:
  1. Builds the Rust binary if necessary.
  2. Runs both extractors (Python `cicflowmeter` package and the Rust
     `cicflowmeter` binary) over every PCAP in the dataset directory with
     repeated trials, measuring wall-clock time, throughput and peak RSS.
  3. Cross-reconciles the generated flow CSVs (feature-by-feature MAE,
     max difference and Pearson correlation).
  4. Writes a timestamped + machine-named result folder containing plots
     (PNG), raw JSON, per-dataset flow CSVs and Markdown reports.

Usage:
    python scripts/run_experiments.py [--data-dir tests/data]
                                      [--output-root experiments]
                                      [--reps 3] [--threads 1]
                                      [--datasets a.pcap,b.pcap]
                                      [--skip-build] [--rust-bin PATH]

Requirements: pandas, numpy, matplotlib, psutil, scapy, cicflowmeter.
"""

from __future__ import annotations

import argparse
import json
import math
import os
import platform
import re
import shutil
import socket
import subprocess
import sys
import time
from datetime import datetime
from pathlib import Path

import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
import numpy as np
import pandas as pd

try:
    import psutil
except ImportError:  # pragma: no cover
    psutil = None

ROOT = Path(__file__).resolve().parent.parent
DRIVER = ROOT / "scripts" / "pcmeter_driver.py"
DEFAULT_RUST_BIN = ROOT / "target" / "release" / "cicflowmeter"

# ---------------------------------------------------------------------------
# Canonical (Rust) -> Python (pip cicflowmeter) feature mapping + groups
# ---------------------------------------------------------------------------

PY_TO_CANON = {
    "src_ip": "Src IP",
    "dst_ip": "Dst IP",
    "src_port": "Src Port",
    "dst_port": "Dst Port",
    "protocol": "Protocol",
    "timestamp": "Timestamp",
    "flow_duration": "Flow Duration",
    "flow_byts_s": "Flow Bytes/s",
    "flow_pkts_s": "Flow Packets/s",
    "fwd_pkts_s": "Fwd Packets/s",
    "bwd_pkts_s": "Bwd Packets/s",
    "tot_fwd_pkts": "Total Fwd Packet",
    "tot_bwd_pkts": "Total Bwd packets",
    "totlen_fwd_pkts": "Total Length of Fwd Packet",
    "totlen_bwd_pkts": "Total Length of Bwd Packet",
    "fwd_pkt_len_max": "Fwd Packet Length Max",
    "fwd_pkt_len_min": "Fwd Packet Length Min",
    "fwd_pkt_len_mean": "Fwd Packet Length Mean",
    "fwd_pkt_len_std": "Fwd Packet Length Std",
    "bwd_pkt_len_max": "Bwd Packet Length Max",
    "bwd_pkt_len_min": "Bwd Packet Length Min",
    "bwd_pkt_len_mean": "Bwd Packet Length Mean",
    "bwd_pkt_len_std": "Bwd Packet Length Std",
    "pkt_len_max": "Packet Length Max",
    "pkt_len_min": "Packet Length Min",
    "pkt_len_mean": "Packet Length Mean",
    "pkt_len_std": "Packet Length Std",
    "pkt_len_var": "Packet Length Variance",
    "fwd_header_len": "Fwd Header Length",
    "bwd_header_len": "Bwd Header Length",
    "fwd_seg_size_min": "Fwd Seg Size Min",
    "fwd_act_data_pkts": "Fwd Act Data Pkts",
    "flow_iat_mean": "Flow IAT Mean",
    "flow_iat_max": "Flow IAT Max",
    "flow_iat_min": "Flow IAT Min",
    "flow_iat_std": "Flow IAT Std",
    "fwd_iat_tot": "Fwd IAT Total",
    "fwd_iat_max": "Fwd IAT Max",
    "fwd_iat_min": "Fwd IAT Min",
    "fwd_iat_mean": "Fwd IAT Mean",
    "fwd_iat_std": "Fwd IAT Std",
    "bwd_iat_tot": "Bwd IAT Total",
    "bwd_iat_max": "Bwd IAT Max",
    "bwd_iat_min": "Bwd IAT Min",
    "bwd_iat_mean": "Bwd IAT Mean",
    "bwd_iat_std": "Bwd IAT Std",
    "fwd_psh_flags": "Fwd PSH Flags",
    "bwd_psh_flags": "Bwd PSH Flags",
    "fwd_urg_flags": "Fwd URG Flags",
    "bwd_urg_flags": "Bwd URG Flags",
    "fin_flag_cnt": "FIN Flag Count",
    "syn_flag_cnt": "SYN Flag Count",
    "rst_flag_cnt": "RST Flag Count",
    "psh_flag_cnt": "PSH Flag Count",
    "ack_flag_cnt": "ACK Flag Count",
    "urg_flag_cnt": "URG Flag Count",
    "ece_flag_cnt": "ECE Flag Count",
    "down_up_ratio": "Down/Up Ratio",
    "pkt_size_avg": "Average Packet Size",
    "init_fwd_win_byts": "FWD Init Win Bytes",
    "init_bwd_win_byts": "Bwd Init Win Bytes",
    "active_max": "Active Max",
    "active_min": "Active Min",
    "active_mean": "Active Mean",
    "active_std": "Active Std",
    "idle_max": "Idle Max",
    "idle_min": "Idle Min",
    "idle_mean": "Idle Mean",
    "idle_std": "Idle Std",
    "fwd_byts_b_avg": "Fwd Bytes/Bulk Avg",
    "fwd_pkts_b_avg": "Fwd Packet/Bulk Avg",
    "bwd_byts_b_avg": "Bwd Bytes/Bulk Avg",
    "bwd_pkts_b_avg": "Bwd Packet/Bulk Avg",
    "fwd_blk_rate_avg": "Fwd Bulk Rate Avg",
    "bwd_blk_rate_avg": "Bwd Bulk Rate Avg",
    "fwd_seg_size_avg": "Fwd Segment Size Avg",
    "bwd_seg_size_avg": "Bwd Segment Size Avg",
    "cwr_flag_count": "CWR Flag Count",
    "subflow_fwd_pkts": "Subflow Fwd Packets",
    "subflow_bwd_pkts": "Subflow Bwd Packets",
    "subflow_fwd_byts": "Subflow Fwd Bytes",
    "subflow_bwd_byts": "Subflow Bwd Bytes",
}

FEATURE_GROUPS = {
    "Flow Duration": "Flow Duration & Counts",
    "Total Fwd Packet": "Flow Duration & Counts",
    "Total Bwd packets": "Flow Duration & Counts",
    "Fwd Packets/s": "Flow Duration & Counts",
    "Bwd Packets/s": "Flow Duration & Counts",
    "Total Length of Fwd Packet": "Payload Length Statistics",
    "Total Length of Bwd Packet": "Payload Length Statistics",
    "Fwd Packet Length Max": "Payload Length Statistics",
    "Fwd Packet Length Min": "Payload Length Statistics",
    "Fwd Packet Length Mean": "Payload Length Statistics",
    "Fwd Packet Length Std": "Payload Length Statistics",
    "Bwd Packet Length Max": "Payload Length Statistics",
    "Bwd Packet Length Min": "Payload Length Statistics",
    "Bwd Packet Length Mean": "Payload Length Statistics",
    "Bwd Packet Length Std": "Payload Length Statistics",
    "Packet Length Min": "Payload Length Statistics",
    "Packet Length Max": "Payload Length Statistics",
    "Packet Length Mean": "Payload Length Statistics",
    "Packet Length Std": "Payload Length Statistics",
    "Packet Length Variance": "Payload Length Statistics",
    "Fwd Header Length": "Payload Length Statistics",
    "Bwd Header Length": "Payload Length Statistics",
    "Average Packet Size": "Payload Length Statistics",
    "Flow Bytes/s": "Throughput & Rates",
    "Flow Packets/s": "Throughput & Rates",
    "Fwd Segment Size Avg": "Throughput & Rates",
    "Bwd Segment Size Avg": "Throughput & Rates",
    "Fwd Seg Size Min": "Throughput & Rates",
    "Fwd Act Data Pkts": "Throughput & Rates",
    "Flow IAT Mean": "Inter-Arrival Times (IAT)",
    "Flow IAT Std": "Inter-Arrival Times (IAT)",
    "Flow IAT Max": "Inter-Arrival Times (IAT)",
    "Flow IAT Min": "Inter-Arrival Times (IAT)",
    "Fwd IAT Total": "Inter-Arrival Times (IAT)",
    "Fwd IAT Mean": "Inter-Arrival Times (IAT)",
    "Fwd IAT Std": "Inter-Arrival Times (IAT)",
    "Fwd IAT Max": "Inter-Arrival Times (IAT)",
    "Fwd IAT Min": "Inter-Arrival Times (IAT)",
    "Bwd IAT Total": "Inter-Arrival Times (IAT)",
    "Bwd IAT Mean": "Inter-Arrival Times (IAT)",
    "Bwd IAT Std": "Inter-Arrival Times (IAT)",
    "Bwd IAT Max": "Inter-Arrival Times (IAT)",
    "Bwd IAT Min": "Inter-Arrival Times (IAT)",
    "Fwd PSH Flags": "TCP Flags",
    "Bwd PSH Flags": "TCP Flags",
    "Fwd URG Flags": "TCP Flags",
    "Bwd URG Flags": "TCP Flags",
    "FIN Flag Count": "TCP Flags",
    "SYN Flag Count": "TCP Flags",
    "RST Flag Count": "TCP Flags",
    "PSH Flag Count": "TCP Flags",
    "ACK Flag Count": "TCP Flags",
    "URG Flag Count": "TCP Flags",
    "CWR Flag Count": "TCP Flags",
    "ECE Flag Count": "TCP Flags",
    "Down/Up Ratio": "TCP Flags",
    "FWD Init Win Bytes": "TCP Window & Bulk",
    "Bwd Init Win Bytes": "TCP Window & Bulk",
    "Fwd Bytes/Bulk Avg": "TCP Window & Bulk",
    "Fwd Packet/Bulk Avg": "TCP Window & Bulk",
    "Fwd Bulk Rate Avg": "TCP Window & Bulk",
    "Bwd Bytes/Bulk Avg": "TCP Window & Bulk",
    "Bwd Packet/Bulk Avg": "TCP Window & Bulk",
    "Bwd Bulk Rate Avg": "TCP Window & Bulk",
    "Subflow Fwd Packets": "Subflow Metrics",
    "Subflow Fwd Bytes": "Subflow Metrics",
    "Subflow Bwd Packets": "Subflow Metrics",
    "Subflow Bwd Bytes": "Subflow Metrics",
    "Active Mean": "Active/Idle Windows",
    "Active Std": "Active/Idle Windows",
    "Active Max": "Active/Idle Windows",
    "Active Min": "Active/Idle Windows",
    "Idle Mean": "Active/Idle Windows",
    "Idle Std": "Active/Idle Windows",
    "Idle Max": "Active/Idle Windows",
    "Idle Min": "Active/Idle Windows",
}

GROUP_ORDER = [
    "Flow Duration & Counts",
    "Payload Length Statistics",
    "Throughput & Rates",
    "Inter-Arrival Times (IAT)",
    "TCP Flags",
    "TCP Window & Bulk",
    "Subflow Metrics",
    "Active/Idle Windows",
]

META_COLS = {"Flow ID", "Label"}
KEY_COLS = ["Src IP", "Dst IP", "Src Port", "Dst Port", "Protocol"]


# ---------------------------------------------------------------------------
# Utilities
# ---------------------------------------------------------------------------

def sanitize(name: str) -> str:
    return re.sub(r"[^A-Za-z0-9_.-]+", "_", name).strip("_")


def fmt_bytes(n: int) -> str:
    for unit in ("B", "KB", "MB", "GB"):
        if abs(n) < 1000.0:
            return f"{n:,.1f} {unit}"
        n /= 1000.0
    return f"{n:,.1f} TB"


def count_packets_fast(path: Path):
    """Fast classic-pcap packet counter; falls back to scapy for pcapng."""
    with open(path, "rb") as f:
        head = f.read(24)
    if len(head) < 24:
        return 0
    magic = head[:4]
    little = magic in (b"\xd4\xc3\xb2\xa1", b"\x4d\x3c\xb2\xa1")
    big = magic in (b"\xa1\xb2\xc3\xd4", b"\xa1\xb2\x3c\x4d")
    if little or big:
        endian = "little" if little else "big"
        n = 0
        with open(path, "rb") as f:
            f.seek(24)
            while True:
                hdr = f.read(16)
                if len(hdr) < 16:
                    break
                caplen = int.from_bytes(hdr[8:12], endian)
                if caplen > 70_000 or caplen < 0:  # defensive
                    break
                f.seek(caplen, 1)
                n += 1
    else:
        # pcapng or unknown -> scapy slow path
        try:
            from scapy.all import PcapReader
            with PcapReader(str(path)) as r:
                n = sum(1 for _ in r)
        except Exception:
            return -1
    return n


def is_probably_pcap(path: Path) -> bool:
    try:
        with open(path, "rb") as f:
            head = f.read(4)
        return head in (
            b"\xd4\xc3\xb2\xa1",
            b"\xa1\xb2\xc3\xd4",
            b"\x4d\x3c\xb2\xa1",
            b"\xa1\xb2\x3c\x4d",
            b"\x0a\x0d\x0d\x0a",
        )
    except OSError:
        return False


# ---------------------------------------------------------------------------
# Process instrumentation
# ---------------------------------------------------------------------------

def measure_process(cmd: list, timeout: int = 3600, poll_s: float = 0.005):
    """Run a subprocess, polling its (and children's) RSS until exit."""
    t0 = time.perf_counter()
    stdout, stderr = [], []
    peak_mb = 0.0
    proc = subprocess.Popen(
        cmd,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0),
    )
    try:
        while proc.poll() is None:
            try:
                if psutil is not None:
                    p = psutil.Process(proc.pid)
                    rss = p.memory_info().rss
                    for c in p.children(recursive=True):
                        try:
                            rss += c.memory_info().rss
                        except Exception:
                            pass
                    peak_mb = max(peak_mb, rss / (1024 * 1024))
            except Exception:
                pass
            time.sleep(poll_s)
        out, err = proc.communicate(timeout=timeout)
        stdout.append(out or b"")
        stderr.append(err or b"")
    except subprocess.TimeoutExpired:
        proc.kill()
        proc.communicate()
        raise
    elapsed = time.perf_counter() - t0

    def _dec(data):
        if isinstance(data, bytes):
            return data.decode("utf-8", errors="replace")
        return data or ""

    return {
        "rc": proc.returncode,
        "elapsed_s": elapsed,
        "peak_rss_mb": peak_mb,
        "stdout": _dec(b"".join(stdout)),
        "stderr": _dec(b"".join(stderr)),
    }


def csv_row_count(path: Path) -> int:
    if not path.exists() or path.stat().st_size == 0:
        return 0
    # Count CSV records, not raw lines: the pip package's writer emits \r\r\n
    # line terminators on Windows, so naive line iteration double-counts rows.
    import csv as _csv
    n = 0
    with open(path, "r", encoding="utf-8", errors="replace", newline="") as f:
        for row in _csv.reader(f):
            if any(str(cell).strip() for cell in row):
                n += 1
    return max(n - 1, 0)


# ---------------------------------------------------------------------------
# Extractors
# ---------------------------------------------------------------------------

def resolve_binary(args) -> Path:
    """Locate the Rust binary, building it if required."""
    if args.rust_bin:
        binary = Path(args.rust_bin)
        if not binary.is_file() and os.name == "nt" and not binary.suffix:
            binary = Path(str(binary) + ".exe")
    else:
        binary = DEFAULT_RUST_BIN
        if os.name == "nt" and not binary.exists():
            binary = Path(str(binary) + ".exe")

    if not binary.exists():
        if args.skip_build:
            print(
                f"ERROR: Rust binary not found at {binary} and --skip-build was set.",
                file=sys.stderr,
            )
            sys.exit(2)
        return build_rust(args)
    return binary


def build_rust(args) -> Path:
    if args.rust_bin:
        binary = Path(args.rust_bin)
        if not binary.is_file() and os.name == "nt" and not binary.suffix:
            binary = Path(str(binary) + ".exe")
    else:
        binary = DEFAULT_RUST_BIN
        if os.name == "nt" and not binary.exists():
            binary = Path(str(binary) + ".exe")
    if binary.exists() and not args.rebuild:
        return binary
    cargo = shutil.which("cargo")
    if not cargo:
        print("ERROR: cargo not found on PATH; cannot build the Rust binary.", file=sys.stderr)
        print("       Build manually (cargo build --release) or pass --rust-bin.", file=sys.stderr)
        sys.exit(2)
    cmd = [cargo, "build", "--release"]
    if os.name == "nt":
        cmd.append("--no-default-features")  # libpcap/wpcap unavailable
    print("Building optimized Rust binary...")
    res = subprocess.run(cmd, cwd=str(ROOT), capture_output=True, text=True)
    if res.returncode != 0:
        print("Rust build FAILED:", file=sys.stderr)
        print(res.stderr[-4000:], file=sys.stderr)
        sys.exit(2)
    if os.name == "nt" and not binary.exists():
        binary = Path(str(binary) + ".exe")
    return binary


def run_rust(binary: Path, pcap: Path, workdir: Path, args, tag: str) -> dict:
    out_dir = workdir / f"rust_{tag}"
    out_dir.mkdir(parents=True, exist_ok=True)
    cmd = [
        str(binary),
        "-r", str(pcap),
        "-o", str(out_dir),
        "--format", "csv",
        "--flow-timeout", str(args.flow_timeout),
        "--activity-timeout", str(args.activity_timeout),
        "--threads", str(args.threads),
        "--min-packets", str(args.min_packets),
        "--label", args.label,
    ]
    if args.rust_compat:
        cmd.append("--compat")
    m = measure_process(cmd)
    out_csv = out_dir / f"{pcap.name}_Flow.csv"
    flows = csv_row_count(out_csv)

    m_packets = re.search(r"Total packets\s*:\s*(\d+)", m["stdout"])
    v_packets = re.search(r"Valid packets\s*:\s*(\d+)", m["stdout"])
    rust_flows = re.search(r"Total flows\s*:\s*(\d+)", m["stdout"])
    packets = int(m_packets.group(1)) if m_packets else None
    valid = int(v_packets.group(1)) if v_packets else None
    reported_flows = int(rust_flows.group(1)) if rust_flows else None

    return {
        "rc": m["rc"],
        "elapsed_s": m["elapsed_s"],
        "peak_rss_mb": m["peak_rss_mb"],
        "stdout": m["stdout"],
        "stderr": m["stderr"],
        "flow_csv": str(out_csv),
        "flows": flows,
        "packets_total": packets or valid,
        "valid_packets": valid,
        "flows_reported": reported_flows,
    }


def run_python(pcap: Path, workdir: Path, tag: str) -> dict:
    out_csv = workdir / f"python_{tag}.csv"
    out_csv.parent.mkdir(parents=True, exist_ok=True)
    cmd = [
        sys.executable, "-u", str(DRIVER), str(pcap), str(out_csv), "--json",
    ]
    m = measure_process(cmd)
    packets = None
    try:
        payload = json.loads(m["stdout"].strip().splitlines()[-1])
        packets = payload.get("packets_examined")
    except Exception:
        pass
    return {
        "rc": m["rc"],
        "elapsed_s": m["elapsed_s"],
        "peak_rss_mb": m["peak_rss_mb"],
        "stdout": m["stdout"],
        "stderr": m["stderr"],
        "flow_csv": str(out_csv),
        "flows": csv_row_count(out_csv),
        "packets_total": packets,
        "valid_packets": packets,
        "flows_reported": None,
    }


# ---------------------------------------------------------------------------
# Feature parity
# ---------------------------------------------------------------------------

def _flow_key(row, key_cols):
    ip_a = str(row[key_cols[0]]).lower()
    ip_b = str(row[key_cols[1]]).lower()
    try:
        pa = int(row[key_cols[2]])
    except Exception:
        pa = 0
    try:
        pb = int(row[key_cols[3]])
    except Exception:
        pb = 0
    try:
        proto = int(row[key_cols[4]])
    except Exception:
        proto = 0
    return (
        (min(ip_a, ip_b), max(ip_a, ip_b)),
        (min(pa, pb), max(pa, pb)),
        proto,
    )


def load_flows(path: Path):
    df = pd.read_csv(path)
    df.columns = [str(c).strip() for c in df.columns]
    return df


def analyze_parity(rust_csv: Path, python_csv: Path) -> dict:
    rust_df = load_flows(rust_csv)
    py_df = load_flows(python_csv)
    py_renamed = py_df.rename(columns={k: v for k, v in PY_TO_CANON.items() if k in py_df.columns})

    rust_df = rust_df.assign(_key=[_flow_key(r, KEY_COLS) for _, r in rust_df.iterrows()])

    # The pip cicflowmeter package stores the frame EtherType (e.g. 2048 for
    # IPv4) in its 'protocol' column instead of the IP L4 protocol number.
    # Hop the Python flow onto the Rust flow sharing the same IP/port pair and
    # adopt that flow's protocol; flows with no unambiguous match stay unmatched.
    rust_key_by_ipport = {}
    for k in set(rust_df["_key"]):
        rust_key_by_ipport.setdefault((k[0], k[1]), set()).add(k[2])

    py_keys = []
    n_ethertype = 0
    for _, r in py_renamed.iterrows():
        k = _flow_key(r, KEY_COLS)
        if k[2] >= 0x0800:  # EtherType stored: real L4 protocols are < 2048
            n_ethertype += 1
            cands = rust_key_by_ipport.get((k[0], k[1]))
            if cands and len(cands) == 1:
                k = (k[0], k[1], next(iter(cands)))
            else:
                k = None
        py_keys.append(k)
    py_renamed = py_renamed.assign(_key=py_keys)
    py_renamed = py_renamed[py_renamed["_key"].notna()]
    py_renamed = py_renamed.reset_index(drop=True)

    rust_s = set(rust_df["_key"])
    py_s = set(py_renamed["_key"])
    matched_keys = rust_s & py_s
    rust_only = rust_s - py_s
    py_only = py_s - rust_s

    rust_m = rust_df[rust_df["_key"].isin(matched_keys)].copy()
    py_m = py_renamed[py_renamed["_key"].isin(matched_keys)].copy()

    # Some extractor runs emit several rows with the same 5-tuple (e.g. a
    # 1-packet observation burst followed by the completed flow). Collapse
    # each side to the longest-surviving flow per key so the merge is 1:1
    # and feature deltas are not polluted by cross-row cartesian products.
    def _longest(df, keycol="_key"):
        df = df.sort_values("Flow Duration", na_position="first", ascending=True)
        return df.drop_duplicates(subset=keycol, keep="last").reset_index(drop=True)

    rust_m = _longest(rust_m)
    py_m = _longest(py_m)
    merged = rust_m.merge(py_m, on="_key", suffixes=("_rust", "_python"), how="inner")

    num_cols = [
        c for c in merged.columns
        if c.endswith("_rust")
        and (c[: -len("_rust")] not in META_COLS)
        and (c[: -len("_rust")] not in KEY_COLS)
        and (c[: -len("_rust")] + "_python") in merged.columns
    ]

    feature_rows = []
    for c in num_cols:
        feat = c[: -len("_rust")]
        group = FEATURE_GROUPS.get(feat, "Other")
        v_rust = pd.to_numeric(merged[c], errors="coerce")
        v_py = pd.to_numeric(merged[c[: -len("_rust")] + "_python"], errors="coerce")
        mask = v_rust.notna() & v_py.notna()
        if mask.sum() == 0:
            continue
        a = v_rust[mask].to_numpy(dtype=float)
        b = v_py[mask].to_numpy(dtype=float)
        diff = np.abs(a - b)
        mae = float(diff.mean())
        maxd = float(diff.max())
        std_a, std_b = float(np.std(a)), float(np.std(b))
        if len(a) > 1 and std_a > 0 and std_b > 0:
            pearson = float(np.corrcoef(a, b)[0, 1])
        else:
            pearson = 1.0
        if math.isnan(pearson):
            pearson = 1.0
        if maxd < 1e-6:
            status = "EXACT"
        elif pearson > 0.999:
            status = "CONCORDANT"
        else:
            status = "DISCREPANCY"
        feature_rows.append({
            "feature": feat,
            "group": group,
            "mae": mae,
            "max_diff": maxd,
            "pearson": pearson,
            "status": status,
            "n_flows": int(mask.sum()),
        })

    n_checked = len(feature_rows)
    n_exact = sum(1 for f in feature_rows if f["status"] == "EXACT")
    n_concord = sum(1 for f in feature_rows if f["status"] == "CONCORDANT")
    n_disc = sum(1 for f in feature_rows if f["status"] == "DISCREPANCY")

    return {
        "rust_flows": int(len(rust_df)),
        "python_flows": int(len(py_renamed)),
        "ethertype_protocol_rows": n_ethertype,
        "matched": int(len(matched_keys)),
        "rust_only": int(len(rust_only)),
        "python_only": int(len(py_only)),
        "features_checked": n_checked,
        "features_exact": n_exact,
        "features_concordant": n_concord,
        "features_discrepancy": n_disc,
        "overall_mae": float(np.mean([f["mae"] for f in feature_rows])) if feature_rows else None,
        "overall_pearson": float(np.mean([f["pearson"] for f in feature_rows])) if feature_rows else None,
        "features": feature_rows,
    }


# ---------------------------------------------------------------------------
# Plotting
# ---------------------------------------------------------------------------

def _agg(reps, key):
    if not reps:
        return None
    arr = np.array([r.get(key) for r in reps], dtype=float)
    return {"mean": float(arr.mean()), "std": float(arr.std())}


def _bar(ax, labels, rust, py, title, ylabel, log=False, fmt="{:,.0f}"):
    x = np.arange(len(labels))
    w = 0.36
    r_bars = ax.bar(x - w / 2, rust, w, label="Rust-CICFlow", color="#0f766e", edgecolor="black", linewidth=0.5)
    p_bars = ax.bar(x + w / 2, py, w, label="CICFlow (Python)", color="#b45309", edgecolor="black", linewidth=0.5)
    ax.set_xticks(x)
    ax.set_xticklabels(labels, rotation=20, ha="right", fontsize=8)
    ax.set_ylabel(ylabel)
    ax.set_title(title)
    if log:
        ax.set_yscale("log")
        ax.grid(True, axis="y", which="both", alpha=0.3)
    else:
        ax.grid(True, axis="y", alpha=0.3)
    ax.legend(fontsize=8)
    if fmt:
        for b in list(r_bars) + list(p_bars):
            h = b.get_height()
            if h and h == h:
                ax.annotate(fmt.format(h), (b.get_x() + b.get_width() / 2, h),
                            ha="center", va="bottom", fontsize=6, rotation=90)
    return ax


def generate_plots(run_dir: Path, raw: dict):
    plots = run_dir / "plots"
    plots.mkdir(parents=True, exist_ok=True)
    datasets = {
        k: v for k, v in raw["datasets"].items()
        if v.get("status") == "ok" and v.get("rust") and v.get("python")
    }
    labels = list(datasets.keys())

    if not labels:
        return

    r_time = [_agg(v["rust"]["reps"], "elapsed_s")["mean"] for v in datasets.values()]
    p_time = [_agg(v["python"]["reps"], "elapsed_s")["mean"] for v in datasets.values()]
    r_rss = [_agg(v["rust"]["reps"], "peak_rss_mb")["mean"] for v in datasets.values()]
    p_rss = [_agg(v["python"]["reps"], "peak_rss_mb")["mean"] for v in datasets.values()]
    r_flow = [float(v["flows_rust"]) for v in datasets.values()]
    p_flow = [float(v["flows_python"]) for v in datasets.values()]
    pkt = [float(v["packets"]) for v in datasets.values()]
    r_tp = [p / t if t else 0.0 for p, t in zip(pkt, r_time)]
    p_tp = [p / t if t else 0.0 for p, t in zip(pkt, p_time)]
    speedup = [rt / pt if pt else 0.0 for rt, pt in zip(p_time, r_time)]

    fig, ax = plt.subplots(figsize=(9, 5), dpi=150)
    _bar(ax, labels, r_tp, p_tp, "End-to-End Throughput (packets/second)", "pkts/s", log=True)
    fig.tight_layout(); fig.savefig(plots / "throughput_comparison.png"); plt.close(fig)

    fig, ax = plt.subplots(figsize=(9, 5), dpi=150)
    _bar(ax, labels, r_time, p_time, "Processing Time (seconds)", "seconds", log=True, fmt="{:.3f}")
    fig.tight_layout(); fig.savefig(plots / "processing_time_comparison.png"); plt.close(fig)

    fig, ax = plt.subplots(figsize=(9, 5), dpi=150)
    _bar(ax, labels, r_rss, p_rss, "Peak Resident Set Size Reference (MB)", "MB", log=True, fmt="{:.1f}")
    fig.tight_layout(); fig.savefig(plots / "memory_comparison.png"); plt.close(fig)

    fig, ax = plt.subplots(figsize=(9, 5), dpi=150)
    speed_colors = ["#065f46" if s >= 1 else "#b91c1c" for s in speedup]
    ax.bar(labels, speedup, color=speed_colors, edgecolor="black", linewidth=0.5)
    for x, s in zip(range(len(labels)), speedup):
        ax.annotate(f"{s:.1f}x", (x, s), ha="center", va="bottom", fontsize=9, fontweight="bold")
    ax.axhline(1.0, color="red", ls="--", lw=1)
    ax.set_xticks(range(len(labels)))
    ax.set_xticklabels(labels, rotation=20, ha="right", fontsize=8)
    ax.set_ylabel("Mean speedup factor (Python time / Rust time)")
    ax.set_title("Rust-CICFlow Speedup over CICFlow (Python)")
    ax.grid(True, axis="y", alpha=0.3)
    fig.tight_layout(); fig.savefig(plots / "speedup_bar.png"); plt.close(fig)

    fig, ax = plt.subplots(figsize=(9, 5), dpi=150)
    _bar(ax, labels, r_flow, p_flow, "Extracted Flow Count", "flows")
    fig.tight_layout(); fig.savefig(plots / "flow_count_comparison.png"); plt.close(fig)

    # Concordance heatmaps (feature group x dataset)
    groups = GROUP_ORDER
    pear = np.full((len(groups), len(labels)), np.nan)
    mae_log = np.full((len(groups), len(labels)), np.nan)
    for j, (ds, v) in enumerate(datasets.items()):
        p = v.get("parity")
        if not p or not p.get("features"):
            continue
        for f in p["features"]:
            g = f["group"]
            if g not in GROUP_ORDER:
                continue
            i = groups.index(g)
            cur_pear = pear[i, j]
            if math.isnan(cur_pear):
                pear[i, j] = f["pearson"]
                mae_log[i, j] = f["mae"] + 1e-12
            else:
                pear[i, j] = min(cur_pear, f["pearson"])  # worst case across features
                mae_log[i, j] = max(mae_log[i, j], f["mae"] + 1e-12)

    fig, ax = plt.subplots(figsize=(max(7, 1.5 * len(labels) + 3), 6), dpi=150)
    im = ax.imshow(pear, aspect="auto", cmap="RdYlGn", vmin=0.9, vmax=1.0)
    ax.set_xticks(range(len(labels)))
    ax.set_xticklabels(labels, rotation=25, ha="right", fontsize=8)
    ax.set_yticks(range(len(groups)))
    ax.set_yticklabels(groups, fontsize=8)
    for i in range(pear.shape[0]):
        for j in range(pear.shape[1]):
            if not math.isnan(pear[i, j]):
                ax.text(j, i, f"{pear[i, j]:.4f}", ha="center", va="center", fontsize=7)
    ax.set_title("Worst-case Pearson correlation by feature group (0.9-1.0 scale)")
    fig.colorbar(im, fraction=0.025, pad=0.02)
    fig.tight_layout(); fig.savefig(plots / "concordance_pearson_heatmap.png"); plt.close(fig)

    fig, ax = plt.subplots(figsize=(max(7, 1.5 * len(labels) + 3), 6), dpi=150)
    im = ax.imshow(mae_log, aspect="auto", cmap="YlOrRd")
    ax.set_xticks(range(len(labels)))
    ax.set_xticklabels(labels, rotation=25, ha="right", fontsize=8)
    ax.set_yticks(range(len(groups)))
    ax.set_yticklabels(groups, fontsize=8)
    for i in range(mae_log.shape[0]):
        for j in range(mae_log.shape[1]):
            if not math.isnan(mae_log[i, j]):
                ax.text(j, i, f"{mae_log[i, j]:.2e}", ha="center", va="center", fontsize=6)
    ax.set_title("Maximum Mean Absolute Error by feature group (log scale)")
    fig.colorbar(im, fraction=0.025, pad=0.02)
    fig.tight_layout(); fig.savefig(plots / "concordance_mae_heatmap.png"); plt.close(fig)


# ---------------------------------------------------------------------------
# Markdown reports
# ---------------------------------------------------------------------------

def md_table(headers, rows, fmt_map=None):
    out = ["| " + " | ".join(headers) + " |", "|" + "|".join(["---"] * len(headers)) + "|"]
    for r in rows:
        cells = []
        for i, cell in enumerate(r):
            s = cell
            if fmt_map and i in fmt_map:
                s = fmt_map[i].format(cell) if cell is not None else "-"
            elif cell is None or (
                isinstance(cell, float) and (math.isnan(cell) or math.isinf(cell))
            ):
                s = "-"
            else:
                s = str(s)
            cells.append(s.replace("|", "\\|"))
        out.append("| " + " | ".join(cells) + " |")
    return "\n".join(out)


def build_report_md(run_dir: Path, raw: dict, out: Path):
    meta = raw["meta"]
    L = []
    L.append("# CICFlow vs Rust-CICFlow — Experimental Comparison Report\n")
    L.append(f"**Platform:** `{meta['host']}`  ")
    L.append(f"**Date/Time:** {meta['timestamp']}  ")
    L.append(f"**System:** {meta['platform']} ({meta['machine']}) — {meta['processor']}  ")
    L.append(f"**Python:** {meta['python']}  ·  **Rust:** {meta['rust_version']}  ")
    L.append(f"**CICFlow (Python) package:** {meta.get('cicflowmeter_version', 'n/a')}  ")
    L.append(f"**Flags:** reps={meta['args']['reps']}, threads={meta['args']['threads']}, "
             f"flow-timeout={meta['args']['flow_timeout']}, activity-timeout={meta['args']['activity_timeout']}, "
             f"min-packets-per-flow={meta['args']['min_packets']}, compat={meta['args']['rust_compat']}\n")

    # Datasets
    L.append("\n---\n\n## 1. Datasets\n")
    rows = []
    for name, v in raw["datasets"].items():
        if v.get("status") == "ok":
            rows.append([name, fmt_bytes(v["bytes"]), str(v["packets"]),
                         str(v.get("flows_rust")), str(v.get("flows_python"))])
        else:
            rows.append([name, fmt_bytes(v["bytes"]), "-", "-", "-"])
    L.append(md_table(["Dataset", "Size", "Packets", "Rust flows", "Python flows"], rows))
    invalid = [n for n, v in raw["datasets"].items() if v.get("status") != "ok"]
    if invalid:
        L.append("\n*Skipped datasets (unreadable/empty): " + ", ".join(
            f"`{n}` ({raw['datasets'][n].get('error', '?')})" for n in invalid) + "*")

    ok = {
        k: v for k, v in raw["datasets"].items()
        if v.get("status") == "ok" and v.get("rust") and v.get("python")
    }

    # Throughput
    L.append("\n---\n\n## 2. Throughput & Processing Time\n")
    rows = []
    for name, v in ok.items():
        rt = _agg(v["rust"]["reps"], "elapsed_s")
        pt = _agg(v["python"]["reps"], "elapsed_s")
        rtp = v["packets"] / rt["mean"] if rt["mean"] > 0 else 0
        ptp = v["packets"] / pt["mean"] if pt["mean"] > 0 else 0
        rows.append([name, str(v["packets"]), rt["mean"], pt["mean"], rtp, ptp, rtp / ptp if ptp else 0])
    L.append(md_table(
        ["Dataset", "Packets", "Rust time (s)", "Python time (s)",
         "Rust (pkts/s)", "Python (pkts/s)", "Speedup"],
        rows,
        fmt_map={2: "{:.4f}", 3: "{:.4f}", 4: "{:,.0f}", 5: "{:,.0f}", 6: "{:.1f}x"},
    ))
    L.append("\n### Throughput comparison\n")
    L.append("![Throughput](plots/throughput_comparison.png)\n")
    L.append("### Processing time comparison\n")
    L.append("![Processing time](plots/processing_time_comparison.png)\n")
    L.append("### Speedup\n")
    L.append("![Speedup](plots/speedup_bar.png)\n")

    # Memory
    L.append("\n---\n\n## 3. Memory\n")
    rows = []
    for name, v in ok.items():
        rt = _agg(v["rust"]["reps"], "peak_rss_mb")
        pt = _agg(v["python"]["reps"], "peak_rss_mb")
        rows.append([name, rt["mean"], pt["mean"], pt["mean"] / rt["mean"] if rt["mean"] else 0])
    L.append(md_table(
        ["Dataset", "Rust peak RSS (MB)", "Python peak RSS (MB)", "Ratio (Python/Rust)"],
        rows, fmt_map={1: "{:.2f}", 2: "{:.2f}", 3: "{:.1f}x"},
    ))
    L.append("\n### Memory comparison\n")
    L.append("![Memory](plots/memory_comparison.png)\n")

    # Flow counts
    L.append("\n---\n\n## 4. Flow Extraction\n")
    L.append("![Flow counts](plots/flow_count_comparison.png)\n")

    # Parity
    L.append("\n---\n\n## 5. Feature Concordance (Rust vs Python)\n")
    rows = []
    for name, v in ok.items():
        p = v.get("parity")
        if not p:
            continue
        rows.append([
            name, p["rust_flows"], p["python_flows"], p["matched"],
            p["features_checked"], p["features_exact"], p["features_concordant"], p["features_discrepancy"],
            p["overall_pearson"] if p["overall_pearson"] is not None else 1.0,
            p["overall_mae"] if p["overall_mae"] is not None else 0.0,
        ])
    if rows:
        L.append(md_table(
            ["Dataset", "Rust flows", "Python flows", "Matched", "Features",
             "Exact", "Concordant", "Discrepant", "Mean r", "Mean MAE"],
            rows, fmt_map={8: "{:.4f}", 9: "{:.4f}"},
        ))
        L.append("\n### Pearson correlation heatmap (by feature group)\n")
        L.append("![Pearson](plots/concordance_pearson_heatmap.png)\n")
        L.append("### MAE heatmap (by feature group)\n")
        L.append("![MAE](plots/concordance_mae_heatmap.png)\n")

    # Per-dataset detailed features
    for name, v in ok.items():
        p = v.get("parity")
        if not p or not p.get("features"):
            continue
        L.append(f"\n---\n\n### Feature-by-feature concordance — `{name}`\n")
        rows = [[
            f["feature"], f["group"], f"n={f['n_flows']}",
            f["mae"], f["max_diff"], f"{f['pearson']:.4f}", f["status"],
        ] for f in p["features"]]
        L.append(md_table(
            ["Feature", "Group", "Flows", "MAE", "Max diff", "Pearson r", "Status"],
            rows, fmt_map={3: "{:.4e}", 4: "{:.4e}"},
        ))

    out.write_text("\n".join(L), encoding="utf-8")


def build_evaluation_md(run_dir: Path, raw: dict, out: Path):
    meta = raw["meta"]
    L = []
    L.append("# Evaluation — CICFlow vs Rust-CICFlow\n")
    L.append(f"Host: **{meta['host']}** · Run: **{meta['timestamp']}**\n")

    L.append("\n## Methodology\n")
    L.append("""
1. **Extractors under test**
   - **CICFlow (Python):** the `cicflowmeter` pip package (uehara implementation), driven through its public
     `FlowSession` API by `scripts/pcmeter_driver.py` (required because the package CLI needs `tcpdump` for
     offline PCAPs and its `toPacketList` finish hook references a scapy API removed in scapy >= 2.5).
   - **Rust-CICFlow:** the optimised `cicflowmeter` release binary built from this repository
     (`cargo build --release`, `--no-default-features` on Windows where libwpcap is absent).
2. **Configuration:** flow timeout 120 s (`120000000` µs), activity timeout 5 s (`5000000` µs),
   min packets per flow 2, CSV export, single worker thread. These match the canonical CIC-IDS settings.
3. **Trials:** every dataset is processed `N` times per extractor (N = configurable `--reps`); a warm-up
   run is executed and discarded before measurement. Reported metrics are the mean across trials.
4. **Instrumentation:** each extractor runs as an isolated subprocess. Wall-clock time is measured with a
   monotonic clock around the subprocess lifetime. Peak Resident Set Size is sampled every ~5 ms (including
   child processes) via `psutil`.
5. **Throughput** = packets in capture ÷ wall-clock time.
6. **Feature parity:** flow CSVs are reconciled on the canonical bidirectional 5-tuple
   (sorted IP pair, sorted port pair, protocol). For every matched flow and every comparable feature,
   Mean Absolute Error (MAE), maximum absolute difference and Pearson correlation are computed.
   *Status*: EXACT if max|diff| < 1e-6, CONCORDANT if r > 0.999, else DISCREPANCY.
""")

    L.append("\n## Environment\n")
    L.append(md_table(
        ["Attribute", "Value"],
        [
            ["Host", meta["host"]],
            ["OS / Platform", f"{meta['platform']} ({meta['machine']})"],
            ["Processor", meta["processor"]],
            ["Python", meta["python"]],
            ["Rust toolchain", meta["rust_version"]],
            ["CICFlow (Python) package", meta.get("cicflowmeter_version", "n/a")],
            ["Scapy", meta.get("scapy_version", "n/a")],
            ["Pandas / Numpy", f"{meta.get('pandas_version', '?')} / {meta.get('numpy_version', '?')}"],
        ],
    ))

    L.append("\n## Limitations & notes\n")
    notes = [
        "- The pip `cicflowmeter` flow extractor only handles IPv4 over Ethernet; packets using IPv6 or non-Ethernet "
        "encapsulations (e.g. `LINUX_SLL2`, present in `real_traffic.pcap`) are silently skipped by the Python "
        "extractor but processed by Rust-CICFlow. Flow counts therefore differ by design on such captures.",
        "- The two extractors use different flow-termination logic (the Python implementation splits a bidirectional "
        "session on FIN and uses a 120 s sliding expiry; the Rust engine tracks a symmetric FIN state machine), so "
        "flow segmentation can differ even when every packet is seen.",
        "- `wireshark_http.pcap` contains a single packet; both extractors therefore emit no meaningful flows under "
        "the default minimum-packets setting.",
        "- Peak RSS is sampled at ~5 ms granularity; short-lived runs can slightly under-report the true "
        "instantaneous peak.",
        "- The Python package stores the frame EtherType (2048 for IPv4) in its `protocol` column rather than the "
        "IP L4 protocol number; feature reconciliation resolves each Python flow to a Rust flow sharing the same "
        "IP/port pair and adopts that flow's protocol.",
        "- The Python package reports `cwr_flag_count` as a copy of `fwd_urg_flags` (an upstream quirk) and "
        "`fwd/bwd_seg_size_avg` as copies of the packet-length means; the comparison maps upstream feature names "
        "onto the canonical 84-name schema where a clear semantic equivalent exists.",
    ]
    errs = [f"- `{n}`: {v.get('error')}" for n, v in raw["datasets"].items() if v.get("status") != "ok"]
    notes.extend(errs)
    L.append("\n".join(notes))
    L.append("\n")
    out.write_text("\n".join(L), encoding="utf-8")


def write_summary_md(run_dir: Path, raw: dict, out: Path):
    meta = raw["meta"]
    ok = {
        k: v for k, v in raw["datasets"].items()
        if v.get("status") == "ok" and v.get("rust") and v.get("python")
    }
    L = [f"# Results summary — `{run_dir.name}`",
         f"\nHost **{meta['host']}** · {meta['timestamp']} · reps={meta['args']['reps']}\n"]
    rows = []
    for name, v in ok.items():
        rt = _agg(v["rust"]["reps"], "elapsed_s")
        pt = _agg(v["python"]["reps"], "elapsed_s")
        rr = _agg(v["rust"]["reps"], "peak_rss_mb")
        rtp = v["packets"] / rt["mean"] if rt["mean"] else 0
        ptp = v["packets"] / pt["mean"] if pt["mean"] else 0
        p = v.get("parity") or {}
        rows.append([
            name, rtp, ptp, (rtp / ptp if ptp else 0),
            rt["mean"], pt["mean"], (pt["mean"] / rt["mean"] if rt["mean"] else 0),
            rr["mean"] if rr else 0, v["packets"],
            p.get("features_concordant", 0), p.get("features_discrepancy", 0),
        ])
    L.append(md_table(
        ["Dataset", "Rust pkts/s", "Python pkts/s", "Speedup", "Rust time", "Python time",
         "Time ratio", "Rust RSS (MB)", "Packets", "Features concordant", "Features discrepant"],
        rows,
        fmt_map={1: "{:,.0f}", 2: "{:,.0f}", 3: "{:.1f}", 4: "{:.3f}", 5: "{:.3f}",
                 6: "{:.1f}x", 7: "{:.2f}"},
    ))
    out.write_text("\n".join(L) + "\n", encoding="utf-8")


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

def gather_meta(args) -> dict:
    pv = platform.python_version()
    sv, cv = "n/a", "n/a"
    try:
        import scapy
        sv = scapy.__version__
    except Exception:
        pass
    try:
        import importlib.metadata as md
        cv = md.version("cicflowmeter")
    except Exception:
        pass
    rust = "n/a"
    try:
        res = subprocess.run(["cargo", "--version"], capture_output=True, text=True)
        rust = res.stdout.strip()
    except Exception:
        pass
    return {
        "host": sanitize(socket.gethostname() or platform.node()),
        "timestamp": datetime.now().strftime("%Y-%m-%d %H:%M:%S"),
        "platform": platform.system() + " " + platform.release(),
        "machine": platform.machine(),
        "processor": platform.processor() or platform.uname().processor,
        "python": pv,
        "rust_version": rust,
        "scapy_version": sv,
        "cicflowmeter_version": cv,
        "pandas_version": pd.__version__,
        "numpy_version": np.__version__,
        "args": vars(args),
    }


def main() -> int:
    parser = argparse.ArgumentParser(description="CICFlow vs Rust-CICFlow experiment harness")
    parser.add_argument("--data-dir", default="tests/data", help="directory containing PCAP datasets")
    parser.add_argument("--output-root", default="experiments", help="root folder for timestamped runs")
    parser.add_argument("--reps", type=int, default=3, help="measurement repetitions per extractor")
    parser.add_argument("--threads", type=int, default=1, help="Rust worker threads")
    parser.add_argument("--flow-timeout", type=int, default=120000000)
    parser.add_argument("--activity-timeout", type=int, default=5000000)
    parser.add_argument("--min-packets", type=int, default=2)
    parser.add_argument("--label", default="NeedManualLabel")
    parser.add_argument("--python-driver", default=None,
                        help="alternate CICFlow driver script (e.g. pcmeter_driver_aligned.py)")
    parser.add_argument("--rust-bin", default=None, help="path to prebuilt cicflowmeter binary")
    parser.add_argument("--rust-compat", action="store_true", help="pass --compat to the Rust binary")
    parser.add_argument("--skip-build", action="store_true", help="do not attempt a Cargo build")
    parser.add_argument("--rebuild", action="store_true", help="force a Cargo rebuild")
    parser.add_argument("--datasets", default=None, help="comma-separated subset of dataset file names")
    args = parser.parse_args()

    data_dir = Path(args.data_dir).resolve()
    if not data_dir.is_dir():
        print(f"ERROR: data directory not found: {data_dir}", file=sys.stderr)
        return 2

    if args.python_driver:
        global DRIVER
        driver = Path(args.python_driver)
        if not driver.is_file():
            driver = ROOT / "scripts" / driver
        DRIVER = driver.resolve()
        print(f"Python driver: {DRIVER}")

    binary = resolve_binary(args)
    if psutil is None:
        print("WARNING: psutil not installed; peak RSS will not be measured.", file=sys.stderr)

    stamp = datetime.now().strftime("%Y%m%d-%H%M%S")
    host = sanitize(socket.gethostname() or platform.node())
    run_dir = Path(args.output_root).resolve() / f"{stamp}_{host}"
    run_dir.mkdir(parents=True, exist_ok=True)
    datasets_dir = run_dir / "datasets"
    datasets_dir.mkdir(parents=True, exist_ok=True)

    print(f"Run directory : {run_dir}")

    meta = gather_meta(args)
    raw = {"meta": meta, "datasets": {}}

    pcap_files = sorted(
        p for p in data_dir.iterdir()
        if p.is_file() and p.suffix.lower() in (".pcap", ".pcapng", ".cap")
    )
    if args.datasets:
        wanted = {s.strip() for s in args.datasets.split(",") if s.strip()}
        pcap_files = [p for p in pcap_files if p.name in wanted]

    if not pcap_files:
        print("No PCAP files found in data directory.", file=sys.stderr)
        return 1

    for idx, pcap in enumerate(pcap_files, 1):
        print(f"\n[{idx}/{len(pcap_files)}] Dataset: {pcap.name}")
        entry = {
            "file": str(pcap),
            "bytes": pcap.stat().st_size,
            "packets": None,
            "status": "ok",
            "error": None,
            "rust": None,
            "python": None,
            "parity": None,
            "flows_rust": None,
            "flows_python": None,
            "notes": [],
        }
        raw["datasets"][pcap.name] = entry

        if not is_probably_pcap(pcap):
            entry["status"] = "error"
            entry["error"] = "not a readable pcap/pcapng file"
            print(f"  ! skipped: {entry['error']}")
            continue

        entry["packets"] = count_packets_fast(pcap)
        if entry["packets"] <= 0:
            entry["status"] = "error"
            entry["error"] = "capture contains no readable packets"
            print(f"  ! skipped: {entry['error']}")
            continue

        work = datasets_dir / sanitize(pcap.stem)
        work.mkdir(parents=True, exist_ok=True)

        # warm-up runs (discarded)
        try:
            run_rust(binary, pcap, work, args, tag="warm")
        except Exception as e:
            print(f"  ! rust warm-up failed: {e}")
        try:
            run_python(pcap, work, tag="warm")
        except Exception as e:
            print(f"  ! python warm-up failed: {e}")

        # measured trials
        rust_reps, py_reps = [], []
        flows_rust, flows_python = [], []
        pkts_rust, pkts_python = [], []

        for rep in range(args.reps):
            r = run_rust(binary, pcap, work, args, tag=str(rep))
            p = run_python(pcap, work, tag=str(rep))
            rust_reps.append({"elapsed_s": r["elapsed_s"], "peak_rss_mb": r["peak_rss_mb"], "flows": r["flows"]})
            py_reps.append({"elapsed_s": p["elapsed_s"], "peak_rss_mb": p["peak_rss_mb"], "flows": p["flows"]})
            flows_rust.append(r["flows"])
            flows_python.append(p["flows"])
            if r.get("packets_total"):
                pkts_rust.append(r["packets_total"])
            if p.get("packets_total"):
                pkts_python.append(p["packets_total"])
            print(f"    rep {rep}: rust {r['elapsed_s']*1000:.2f} ms / {r['flows']} flows | "
                  f"python {p['elapsed_s']*1000:.2f} ms / {p['flows']} flows")

        entry["rust"] = {
            "reps": rust_reps,
            "flows_mean": float(np.mean(flows_rust)),
            "packets_mean": float(np.mean(pkts_rust)) if pkts_rust else entry["packets"],
        }
        entry["python"] = {
            "reps": py_reps,
            "flows_mean": float(np.mean(flows_python)),
            "packets_mean": float(np.mean(pkts_python)) if pkts_python else entry["packets"],
        }
        entry["flows_rust"] = int(round(entry["rust"]["flows_mean"]))
        entry["flows_python"] = int(round(entry["python"]["flows_mean"]))

        # feature parity on final trial output
        last_rust_csv = work / f"rust_{args.reps - 1}" / f"{pcap.name}_Flow.csv"
        last_py_csv = work / f"python_{args.reps - 1}.csv"
        if last_rust_csv.exists() and last_py_csv.exists() and entry["flows_rust"] > 0 and entry["flows_python"] > 0:
            try:
                entry["parity"] = analyze_parity(last_rust_csv, last_py_csv)
                p = entry["parity"]
                print(f"    parity: matched {p['matched']} flows · features checked {p['features_checked']} · "
                      f"exact {p['features_exact']} · concordant {p['features_concordant']} · "
                      f"discrepant {p['features_discrepancy']}")
                if p.get("features"):
                    pd.DataFrame(p["features"]).to_csv(work / f"parity_{sanitize(pcap.stem)}.csv", index=False)
            except Exception as e:
                print(f"    ! parity analysis failed: {e}")
                entry["parity"] = None

    (run_dir / "raw_results.json").write_text(json.dumps(raw, indent=2, default=str), encoding="utf-8")

    print("\nGenerating plots and reports...")
    try:
        generate_plots(run_dir, raw)
    except Exception:
        import traceback
        traceback.print_exc()
        print("WARNING: plot generation failed.", file=sys.stderr)
    build_report_md(run_dir, raw, run_dir / "report.md")
    build_evaluation_md(run_dir, raw, run_dir / "evaluation.md")
    write_summary_md(run_dir, raw, run_dir / "results_summary.md")

    print(f"\nExperiment complete.")
    print(f"  Results: {run_dir}")
    print(f"  Report : {run_dir / 'report.md'}")
    return 0


if __name__ == "__main__":
    sys.exit(main())