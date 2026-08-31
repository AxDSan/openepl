#!/bin/bash
# Compile every OpenEPL code sample in the documentation.
#
# A sample that does not build is worse than no sample: it is the first thing a
# newcomer copies, and it teaches them the language wrong. Fenced blocks with
# no language tag are treated as OpenEPL and must contain `module` to be
# considered a whole program — fragments are skipped deliberately.
set -uo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
OPENEPL="${OPENEPL:-$ROOT/target/release/openepl}"
[ -x "$OPENEPL" ] || OPENEPL="$ROOT/target/debug/openepl"
[ -x "$OPENEPL" ] || { echo "build openepl first (cargo build)" >&2; exit 1; }

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

pass=0
fail=0

for doc in docs-site/src/*.md README.md docs/*.md; do
    [ -f "$doc" ] || continue
    # Split the file into fenced blocks, keeping only untagged ones that look
    # like a whole module.
    python3 - "$doc" "$WORK" <<'PY'
import re, sys, os
doc, work = sys.argv[1], sys.argv[2]
text = open(doc, encoding='utf-8').read()
stem = os.path.basename(doc).replace('.', '_')
for i, m in enumerate(re.finditer(r'^```([a-zA-Z]*)\n(.*?)^```', text, re.S | re.M)):
    lang, body = m.group(1), m.group(2)
    if lang not in ('', 'openepl', 'oir'):
        continue
    if not re.search(r'^module\s+\w+', body, re.M):
        continue
    open(os.path.join(work, f'{stem}_{i}.oir'), 'w', encoding='utf-8').write(body)
PY
done

shopt -s nullglob
for sample in "$WORK"/*.oir; do
    name="$(basename "$sample" .oir)"
    if out=$("$OPENEPL" build "$sample" -o "$WORK/$name.bin" 2>&1); then
        printf '  %-44s PASS\n' "$name"
        pass=$((pass + 1))
    else
        printf '  %-44s FAIL\n' "$name"
        sed 's/^/      /' <<<"$out" | head -6
        fail=$((fail + 1))
    fi
done

echo
echo "  $pass sample(s) compiled, $fail failed"
[ "$fail" -eq 0 ]
