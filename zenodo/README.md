# Zenodo Release Package — DEF CON 26 CTF Capture + Rust-CICFlow Flows

This folder is a self-contained Zenodo publication package. Everything here is
structured for either (a) browser upload at https://zenodo.org/deposit/new
(recommended for the pcap subset files) or (b) scripted upload via the Zenodo
REST API (`zenodo_upload.py`) for the >50 GB artifacts that the web uploader
chokes on.

## Files to publish

### Record A — DEF CON 26 CTF packet capture + subsets
| File | Size | SHA-256 (see `SHA256SUMS.txt`) | Location on this machine |
|---|---|---|---|
| `DEF CON 26 ctf packet captures.pcap` (full 49 GB capture: 156,114,913 packets) | 52.92 GB | `feca600b81f02e8b…` | `C:\Users\bloodraven\AppData\Local\Temp\opencode\defcon26\defcon26_extracted\` |
| `defcon26_head.pcapng` (packets [0, 200000)) | 25.7 MB | `1ae49a86…` | `experiments\20260923-020154_Local_Computer\subsets\` |
| `defcon26_mid.pcapng` (packets [77,957,456…78,157,456)) | 101.3 MB | `9869faece976e59d…` | same |
| `defcon26_tail.pcapng` (last 200,000) | 48.0 MB | `25051f3753e56…` | same |

The full capture is **52.92 GB — above the 50 GB per-file Zenodo limit**. It has
already been byte-split (SHA-256 verified stream, parent hash matches
`SHA256SUMS.txt`) into three parts under
`C:\Users\bloodraven\AppData\Local\Temp\opencode\defcon26\zenodo_parts\`:

| Part file | Size | SHA-256 (see `SHA256SUMS_parts.txt` there) |
|---|---|---|
| `DEF_CON_26_ctf_packet_captures.part01of03` | 21.47 GB | `2f806fe509d74916…` |
| `DEF_CON_26_ctf_packet_captures.part02of03` | 21.47 GB | `35a731117f8c601c…` |
| `DEF_CON_26_ctf_packet_captures.part03of03` | 9.97 GB | `5f7b5619a870c87a…` |

Consumers reassemble with `copy /b part01of03+part02of03+part03of03 out.pcap`
(or `cat DEF_CON_26_ctf_packet_captures.part* > out.pcap`) and verify against
the parent digest `feca600b81f02e8b…`.

### Record B — Extracted 84-feature CIC-IDS-style flows
| File | Size | SHA-256 | Location |
|---|---|---|---|
| `full.pcap_Flow.csv` (7,798,789 flows; Rust-CICFlow, single-file full-capture run) | 4.58 GB | `b6d44820ad8a1cc4…` | `C:\Users\bloodraven\AppData\Local\Temp\opencode\defcon26\full_rust_out\` |
| subset flow CSVs, Rust + Python reference, per subset (6 files, 4–17 MB each) | ~100 MB | not pinned | `experiments\20260923-020154_Local_Computer\datasets\defcon26_{head,mid,tail}\{rust_0\*.csv,python_0.csv}` |
| Provenance docs: `analysis.md`, `results_summary_corrected.md`, `evaluation.md` | small | — | `experiments\20260923-020154_Local_Computer\` |

Uploading actual flow CSVs + provenance documents lets readers reproduce the
parity analysis (92.5% flow-population agreement; per-flow feature math exact
on 99.997% of same-packet-count flows) rather than only trusting the write-up.

> Note: `*.csv` / `*.json` are `.gitignore`d repo-wide, so the flow CSVs and
> raw results stay local; they are only published here (Zenodo) — this is
> intentional and matches the repo's data-handling policy.

## Metadata
- `zenodo.json` — structured metadata conforming to the Zenodo deposit schema
  (title, HTML description, creators, keywords, license `CC-BY-4.0`, related
  identifiers: this GitHub repo `isSupplementedBy` and the DEF CON media source
  `isDerivedFrom`). Ready to paste (JSON Upload → deposit metadata or
  programmatically).
- `SHA256SUMS.txt` — the manifest, to be included verbatim as a record file.

## Publish in 1 minute (browser, this machine)
1. Log in at https://zenodo.org/deposit/new (upload type: dataset).
2. Drag the files listed per record above (from the "Location" column).
3. Paste the "description" block from `zenodo.json` into the Description field;
   keep the same keywords/license/access-right.
4. Verify each uploaded file's listed size; Zenodo computes checksums client-side too.
5. Publish. For the 52.9 GB pcap, run `python zenodo/zenodo_split.py` first and
   upload the generated `.part` files + `SHA256SUMS.txt`.

## Or: scripted upload (recommended for the big-file records)
```bash
# one-time: create a personal access token with deposit:write scope at
#   https://zenodo.org/account/settings/applications/tokens/new/
set ZENODO_TOKEN=<token>
python zenodo/zenodo_upload.py --files "location1;location2;..." \
       --metadata zenodo/zenodo.json --reconstruct-note "..."   # no --publish → review, then publish via --publish
```
`zenodo_upload.py` uses the bucket streaming API (blank refreshed if a PUT is
interrupted; retry-safe, no duplicate rows).

## Licensing & attribution
- Data (`DEF CON 26 ctf packet captures.pcap` + subsets): **CC-BY-4.0** gives the
  correct attribution to the original DEF CON capture and the derived flow
  artifacts; the upstream rar archive stays at media.defcon.org.
- Software provenance documents: MIT/Apache-2.0 per the repository LICENSE.
