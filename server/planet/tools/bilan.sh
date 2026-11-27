#!/bin/sh
# The bilan of a version on thirty seeds, once, at the end of a round:
#
#     tools/bilan.sh v12 OUT_DIR [settings.ron]
#
# writes OUT_DIR/bilan-v12.json (release build, every core).
set -eu
version=$1
out=$(realpath "$2")
settings=${3:+--settings $(realpath "$3")}
cd "$(dirname "$0")/../.."
cage=$(command -v soul-cap || true)
# shellcheck disable=SC2086
exec $cage cargo run --release -q -p planet --example bilan -- \
    $settings --seeds 1-30 --version "$version" --out "$out/bilan-$version.json"
