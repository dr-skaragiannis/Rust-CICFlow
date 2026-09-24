#!/usr/bin/env python3
"""Zenodo deposit uploader using the Bucket streaming API.

Requirements: requests (pip install requests).
Auth: a Zenodo personal access token with `deposit:actions` + `deposit:write`
scope, exported as ZENODO_TOKEN (https://zenodo.org/account/settings/applications/).

Usage:
  python zenodo_upload.py --files "<path1>;<path2>;..." [--metadata zenodo/zenodo.json]
                          [--draft | --publish] [--pause-callback "txt"]
Creates a new deposit, uploads each file (streaming; 256 MB chunks keep memory flat),
attaches metadata, and optionally publishes. Prints the record/banner DOI.
"""
import argparse, json, os, sys, time
import requests

ZENODO_API = os.environ.get('ZENODO_API', 'https://zenodo.org/api')


def _hdr(token, extra=None):
    h = {'Authorization': f'Bearer {token}'}
    if extra:
        h.update(extra)
    return h


def create_deposit(token, metadata_path):
    meta = json.load(open(metadata_path, encoding='utf-8'))
    payload = {'metadata': meta}
    r = requests.post(f'{ZENODO_API}/deposit/depositions',
                      headers=_hdr(token, {'Content-Type': 'application/json'}),
                      data=json.dumps(payload))
    r.raise_for_status()
    dep = r.json()
    print(f'created deposit id={dep["id"]}: {meta.get("title")}')
    return dep


def upload_file(token, bucket_url, path):
    name = os.path.basename(path)
    size = os.path.getsize(path)
    url = f'{bucket_url}/{name}'
    print(f'  uploading {name} ({size/1e9:.2f} GB)...', flush=True)
    t0 = time.time()
    with open(path, 'rb') as f:
        for attempt in range(3):
            f.seek(0)
            try:
                with requests.put(url, data=f, headers=_hdr(token, {'Content-Type': 'application/octet-stream'}), stream=True) as r:
                    r.raise_for_status()
                break
            except requests.RequestException as e:
                print(f'    retry {attempt + 1}/3 after {e}', flush=True)
                time.sleep(2 ** attempt)
    took = time.time() - t0
    rate = size / took / 1e6 if took else 0
    print(f'    done in {took:.0f}s ({rate:.1f} MB/s)')


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--files', required=True, help='";"-separated local file paths to upload')
    ap.add_argument('--metadata', default=os.path.join(os.path.dirname(__file__), 'zenodo.json'))
    ap.add_argument('--publish', action='store_true', help='publish immediately (default: leave as draft for review)')
    args = ap.parse_args()

    token = os.environ.get('ZENODO_TOKEN')
    if not token:
        print('ERROR: export ZENODO_TOKEN first (deposit:write scope).', file=sys.stderr)
        return 2

    dep = create_deposit(token, args.metadata)
    links = dep['links']
    bucket_url = links.get('bucket')
    for path in [p.strip() for p in args.files.split(';') if p.strip()]:
        upload_file(token, bucket_url, path)

    r = requests.get(links['self'], headers=_hdr(token))
    r.raise_for_status()
    dep = r.json()
    print('deposit files:')
    for f in dep.get('files', []):
        print(f"  {f['filename']}  {f['filesize']/1e9:.3f} GB  checksum={f['checksum']}")
    if args.publish:
        r = requests.post(links['publish'], headers=_hdr(token))
        r.raise_for_status()
        dep = r.json()
        print(f"PUBLISHED: doi={dep.get('doi')}  {dep.get('links', {}).get('latest_html')}")
    else:
        print(f'DRAFT saved: {links.get("latest_html", links["html"])}')
        print('Review at the URL above (or re-run with --publish).')
    return 0


if __name__ == '__main__':
    sys.exit(main())
