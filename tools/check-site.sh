#!/bin/bash
# Check an assembled documentation site before it is published.
#
# A site that builds is not a site that works: the failure that actually
# reaches people is a link or an image that resolves to nothing, and neither
# mdBook nor the landing page's HTML will say a word about it.
set -uo pipefail

SITE="${1:-_site}"
[ -d "$SITE" ] || { echo "no such directory: $SITE" >&2; exit 2; }

python3 - "$SITE" <<'PY'
import os, re, sys

site = sys.argv[1]
missing, checked = [], 0

for dirpath, _, files in os.walk(site):
    for f in files:
        if not f.endswith(('.html', '.css')):
            continue
        path = os.path.join(dirpath, f)
        text = open(path, encoding='utf-8', errors='ignore').read()
        refs = re.findall(r'(?:href|src)="([^"]+)"', text)
        refs += re.findall(r'url\("([^"]+)"\)', text)
        for ref in refs:
            if ref.startswith(('http://', 'https://', '//', 'mailto:', 'data:', '#')):
                continue
            ref = ref.split('#')[0].split('?')[0]
            if not ref:
                continue
            if ref.startswith('/'):
                # Deployed under a path prefix; check what follows the repo name.
                parts = ref.strip('/').split('/', 1)
                ref = parts[1] if len(parts) > 1 else ''
                target = os.path.join(site, ref) if ref else site
            else:
                target = os.path.normpath(os.path.join(dirpath, ref))
            checked += 1
            if not (os.path.exists(target) or os.path.exists(target + 'index.html')
                    or os.path.exists(os.path.join(target, 'index.html'))):
                missing.append((os.path.relpath(path, site), ref))

for src, ref in missing[:20]:
    print(f"  MISSING  {ref}\n           referenced by {src}")
print(f"\n  {checked} reference(s) checked, {len(missing)} missing")
sys.exit(1 if missing else 0)
PY
