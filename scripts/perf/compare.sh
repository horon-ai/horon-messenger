#!/usr/bin/env bash
set -euo pipefail

usage() {
  echo "usage: bash scripts/perf/compare.sh <before.json> <after.json>" >&2
}

if [ "$#" -ne 2 ]; then
  usage
  exit 2
fi

BEFORE="$1"
AFTER="$2"

for path in "$BEFORE" "$AFTER"; do
  if [ ! -f "$path" ]; then
    echo "compare.sh: file not found: $path" >&2
    exit 1
  fi
done

python3 - "$BEFORE" "$AFTER" <<'PY'
import json
import sys

before_path, after_path = sys.argv[1], sys.argv[2]


def load(path):
    try:
        with open(path, encoding="utf-8") as handle:
            return json.load(handle)
    except (OSError, json.JSONDecodeError) as error:
        print(f"compare.sh: cannot read {path}: {error}", file=sys.stderr)
        sys.exit(1)


def flat(data, prefix=""):
    values = {}
    for key, value in data.items():
        name = f"{prefix}{key}"
        if isinstance(value, dict):
            values.update(flat(value, f"{name}."))
        elif isinstance(value, (int, float)) and not isinstance(value, bool):
            values[name] = float(value)
    return values


before = load(before_path)
after = load(after_path)
b_flat = flat(before)
a_flat = flat(after)

rows = []
for name in sorted(set(b_flat) | set(a_flat)):
    if name.endswith(".n"):
        continue
    b = b_flat.get(name)
    a = a_flat.get(name)
    if b is None or a is None:
        delta = "n/a"
    else:
        delta = f"{a - b:+.4f}"
    rows.append((name, b, a, delta))

width = max((len(name) for name, *_ in rows), default=6)
header = f"{'metric':<{width}}  {'before':>14}  {'after':>14}  {'delta':>12}"
print(f"# compare: before={before.get('label', before_path)} after={after.get('label', after_path)}")
print(header)
print("-" * len(header))
for name, b, a, delta in rows:
    b_txt = "n/a" if b is None else f"{b:.4f}"
    a_txt = "n/a" if a is None else f"{a:.4f}"
    print(f"{name:<{width}}  {b_txt:>14}  {a_txt:>14}  {delta:>12}")
PY
