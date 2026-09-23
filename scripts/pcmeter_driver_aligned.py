#!/usr/bin/env python3
"""
Driver for the pip `cicflowmeter` package, re-parameterized to the canonical
CIC-IDS time constants before any flow is created:

  - EXPIRED_UPDATE   240 s -> 120 s   (flow/inactivity split)
  - ACTIVE_TIMEOUT   0.005 s -> 5 s   (active/idle window threshold)
  - CLUMP_TIMEOUT    0.001 s -> 5 s   (bulk clump / subflow trigger)

flow_session binds EXPIRED_UPDATE at import time, so both module namespaces
must be patched. This isolates how much of the feature discrepancy on real
captures is caused by the uehara package's non-canonical hardcoded constants
rather than by feature math.

Usage:
    python pcmeter_driver_aligned.py <input.pcap> <output.csv> [--json]
"""

import argparse
import json
import os
import sys


def run(input_pcap: str, output_csv: str) -> dict:
    import cicflowmeter.constants as constants
    import cicflowmeter.flow_session as flow_session

    constants.EXPIRED_UPDATE = 120
    constants.ACTIVE_TIMEOUT = 5
    constants.CLUMP_TIMEOUT = 5
    flow_session.EXPIRED_UPDATE = 120
    # flow.py reads ACTIVE_TIMEOUT / CLUMP_TIMEOUT through the constants module
    # at call time, so patching `constants` above covers those.

    from cicflowmeter.flow_session import FlowSession
    from scapy.all import PcapReader

    if os.path.exists(output_csv):
        os.remove(output_csv)

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
                continue

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
            "CICFlow (Python, canonical constants) driver: examined {} packets -> {}".format(
                result["packets_examined"], result["output"]
            )
        )
    return 0


if __name__ == "__main__":
    sys.exit(main())