#!/usr/bin/env python3
"""
Driver for the pip `cicflowmeter` (uehara) package used as the CICFlow reference
extractor inside the experiment harness.

The package's CLI depends on `tcpdump` for offline captures (unavailable on
Windows) and its `FlowSession.toPacketList` hook references a scapy API that
was removed in scapy >= 2.5. This driver re-implements the exact dispatch loop
of the package against its public `FlowSession`/`Flow` API so both tools can be
run as plain subprocesses with identical wall-clock/memory instrumentation.

Usage:
    python pcmeter_driver.py <input.pcap> <output.csv> [--json]
"""

import argparse
import json
import os
import sys


def run(input_pcap: str, output_csv: str) -> dict:
    from cicflowmeter.flow_session import FlowSession
    from scapy.all import PcapReader

    if os.path.exists(output_csv):
        os.remove(output_csv)

    # Route the package's writer to our CSV destination (same mechanism the
    # package CLI uses via create_sniffer()).
    FlowSession.output_mode = "csv"
    FlowSession.output = output_csv

    session = FlowSession()
    packets_examined = 0
    with PcapReader(input_pcap) as reader:
        for pkt in reader:
            packets_examined += 1
            try:
                session.on_packet_received(pkt)
            except Exception:
                # Mirrors the package's own defensive per-packet handling.
                continue

    # Flush any flows still resident in the session (equivalent to the package's
    # toPacketList() finish hook), then close the writer file handle.
    session.garbage_collect(None)
    if getattr(session, "output_writer", None) is not None:
        try:
            session.output_writer.__del__()
        except Exception:
            pass

    return {"packets_examined": packets_examined, "output": output_csv}


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("input_pcap")
    parser.add_argument("output_csv")
    parser.add_argument("--json", action="store_true")
    args = parser.parse_args()

    result = run(args.input_pcap, args.output_csv)
    if args.json:
        print(json.dumps(result))
    else:
        print(
            "CICFlow (Python) driver: examined {} packets -> {}".format(
                result["packets_examined"], result["output"]
            )
        )
    return 0


if __name__ == "__main__":
    sys.exit(main())