#!/usr/bin/env bash
# Fetch the third-party code the editor builds on (pinned to the commits it was developed against) and create
# the Python environment. Then run prepare.py with your own ROM and a gloveworld checkout.
set -euo pipefail
cd "$(dirname "$0")"

fetch() {  # url dir commit: shallow fetch of one exact commit
  local url=$1 dir=$2 rev=$3
  if [ ! -d "$dir/.git" ]; then
    git init -q "$dir"
    git -C "$dir" remote add origin "$url"
  fi
  if [ "$(git -C "$dir" rev-parse -q --verify HEAD 2>/dev/null)" != "$rev" ]; then
    git -C "$dir" fetch -q --depth 1 origin "$rev"
    git -C "$dir" checkout -q "$rev"
  fi
  echo "$dir @ ${rev:0:7}"
}
mkdir -p vendor
fetch https://github.com/magcius/noclip.website vendor/noclip fd973ff240233c63883750a557c62baf6ff2a6c7
fetch https://github.com/naclomi/libgarib vendor/libgarib cf879296afc5108867de03facbca428923618f29

[ -x .venv/bin/python ] || python3 -m venv .venv
.venv/bin/pip install -q -r vendor/libgarib/requirements.txt -r requirements.txt
echo "Next: .venv/bin/python prepare.py --rom /path/to/glover.z64 --gloveworld /path/to/gloveworld"
