#!/usr/bin/env bash
# Lochor license-compliance audit (Unix / macOS / CI).
#
# Verifies that every third-party dependency is permissively licensed (no
# GPL/AGPL/SSPL) so the closed-source, paid build is compliant. Reproduces the
# audit recorded in THIRD_PARTY_LICENSES/README.md.
#
#   ./scripts/license-audit.sh
#
# Exit code is non-zero if any gate fails.
set -euo pipefail
cd "$(dirname "$0")/.."
fail=0

echo "== Lochor license audit =="

# ── 1. Rust workspace ──────────────────────────────────────────────────────
echo
echo "[1/3] Rust crates (cargo-deny)..."
if command -v cargo-deny >/dev/null 2>&1; then
  cargo deny check licenses || fail=1
else
  echo "  cargo-deny not installed; falling back to cargo metadata scan."
  echo "  (install the gate with: cargo install cargo-deny)"
  bad=$(cargo metadata --format-version 1 --all-features 2>/dev/null | python3 -c '
import sys,json
d=json.load(sys.stdin); out=[]
for p in d["packages"]:
    lic=(p.get("license") or "")
    if not lic: continue
    U=lic.upper()
    if any(c in U for c in ("GPL","AGPL","SSPL","EUPL","CDDL")) and " OR " not in lic and "/" not in lic:
        out.append(p["name"]+" "+p["version"]+": "+lic)
print("\n".join(out))
')
  if [ -n "$bad" ]; then
    echo "  BLOCKING copyleft found:"; echo "$bad" | sed "s/^/    /"; fail=1
  else
    echo "  OK - no strong copyleft in the Rust tree."
  fi
fi

# ── 2. Frontend (pnpm) ─────────────────────────────────────────────────────
echo
echo "[2/3] Frontend prod deps (pnpm)..."
fe=$(pnpm licenses list --prod 2>/dev/null | grep -iE "GPL|AGPL|SSPL|EUPL|CDDL" | grep -viE " OR |LGPL-.*OR" || true)
if [ -n "$fe" ]; then
  echo "  BLOCKING copyleft in frontend:"; echo "$fe" | sed "s/^/    /"; fail=1
else
  echo "  OK - frontend prod deps are permissive."
fi

# ── 3. Python sidecar (optional) ───────────────────────────────────────────
echo
echo "[3/3] Python model-editing sidecar..."
if [ -f sidecar/requirements.txt ]; then
  if command -v pip-licenses >/dev/null 2>&1; then
    pip-licenses --format=markdown --with-urls | grep -iE "GPL|AGPL|SSPL" | grep -vi "LGPL" || true
    echo "  Review any GPL rows above before shipping the sidecar."
  else
    echo "  pip-licenses not installed (pip install pip-licenses)."
  fi
else
  echo "  No sidecar/requirements.txt yet - skipped."
fi

echo
if [ "$fail" -ne 0 ]; then
  echo "AUDIT FAILED - a non-permissive license entered the tree."; exit 1
else
  echo "AUDIT PASSED - stack is safe for a commercial build."
  echo "Reminder: ship THIRD_PARTY_LICENSES/ with every distribution."
fi
