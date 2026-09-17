"""
CICFlowMeter Python Wrapper (Powered by High-Speed Rust Engine)

Provides easy Python bindings and Pandas/Polars integration to analyze PCAPs and extract
the 84 canonical flow features in milliseconds.
"""

import subprocess
import os
import sys
from pathlib import Path
from typing import Optional, Union, List

# Locate the compiled Rust binary
DEFAULT_BINARY_PATH = Path(__file__).parent.parent / "target" / "release" / "cicflowmeter"

class CICFlowMeter:
    def __init__(self, binary_path: Optional[Union[str, Path]] = None):
        if binary_path is None:
            self.binary_path = DEFAULT_BINARY_PATH
        else:
            self.binary_path = Path(binary_path)

        if not self.binary_path.exists():
            # Try finding in PATH
            import shutil
            which_path = shutil.which("cicflowmeter")
            if which_path:
                self.binary_path = Path(which_path)
            else:
                raise FileNotFoundError(
                    f"CICFlowMeter binary not found at '{self.binary_path}'. "
                    f"Please run 'cargo build --release' in the cicflowmeter-rust directory first."
                )

    def process_pcap(
        self,
        input_path: Union[str, Path],
        output_dir: Union[str, Path] = "./output",
        output_format: str = "csv",
        flow_timeout: int = 120_000_000,
        activity_timeout: int = 5_000_000,
        threads: Optional[int] = None,
        label: str = "NeedManualLabel",
        compat: bool = False,
    ) -> Path:
        """
        Runs the Rust CICFlowMeter on an input PCAP/PCAPNG file or directory of PCAP files.
        Returns the Path to the output directory or generated CSV/JSON file.
        """
        input_p = Path(input_path)
        output_d = Path(output_dir)
        output_d.mkdir(parents=True, exist_ok=True)

        cmd = [
            str(self.binary_path),
            "-r", str(input_p),
            "-o", str(output_d),
            "--format", output_format,
            "--flow-timeout", str(flow_timeout),
            "--activity-timeout", str(activity_timeout),
            "--label", label,
        ]

        if threads is not None:
            cmd.extend(["--threads", str(threads)])

        if compat:
            cmd.append("--compat")

        result = subprocess.run(cmd, capture_output=True, text=True)
        if result.returncode != 0:
            raise RuntimeError(f"CICFlowMeter execution failed: {result.stderr}")

        if input_p.is_file():
            ext = ".csv" if output_format == "csv" else f".{output_format}"
            out_file = output_d / f"{input_p.name}_Flow{ext}"
            return out_file
        else:
            return output_d

    def to_polars(self, input_pcap: Union[str, Path], **kwargs):
        """Processes PCAP and returns a Polars DataFrame."""
        import polars as pl
        out_csv = self.process_pcap(input_pcap, output_format="csv", **kwargs)
        return pl.read_csv(out_csv)

    def to_pandas(self, input_pcap: Union[str, Path], **kwargs):
        """Processes PCAP and returns a Pandas DataFrame."""
        import pandas as pd
        out_csv = self.process_pcap(input_pcap, output_format="csv", **kwargs)
        return pd.read_csv(out_csv)

if __name__ == "__main__":
    if len(sys.argv) < 2:
        print("Usage: python cicflowmeter.py <input.pcap> [out_dir]")
        sys.exit(1)

    meter = CICFlowMeter()
    out = meter.process_pcap(sys.argv[1], sys.argv[2] if len(sys.argv) > 2 else "./output")
    print(f"Generated flow dataset: {out}")
