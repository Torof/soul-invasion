#!/usr/bin/env bash
# Exports the game headless and prints what it weighs (D39: measured, so
# the shipped size has a number at every change).
#
#   client/tools/export_size.sh [preset]      (default: Linux)
#
# Always: the pack (what the game's data weigh, shipped beside or inside
# the engine) and its heaviest folders. When the export templates of this
# Godot version are installed (Editor > Manage Export Templates, or the
# .tpz from godotengine.org), the whole game too: engine and pack.
# Writes under client/export/, which git ignores. Never opens a window.
set -euo pipefail

client="$(cd "$(dirname "$0")/.." && pwd)"
preset="${1:-Linux}"
out="$client/export"
mkdir -p "$out"

mb() { awk -v b="$1" 'BEGIN { printf "%.1f MB", b / 1048576 }'; }
size_of() { stat -c %s "$1"; }

# A fresh checkout has nothing imported yet; done, this takes seconds.
godot --headless --path "$client" --import >/dev/null 2>&1 || true

echo "== pack ($preset)"
godot --headless --path "$client" --export-pack "$preset" "$out/soul-invasion.pck" >"$out/export-pack.log" 2>&1 \
  || { echo "pack export failed, see $out/export-pack.log"; exit 1; }
pack=$(size_of "$out/soul-invasion.pck")
echo "soul-invasion.pck: $(mb "$pack")"

# What weighs, from the .import files (the art folders are links into
# content/art, hence -L): each imported texture under its
# source's folder (the pack holds the imported files, under .godot/).
echo "-- imported art by source folder"
find -L "$client" -name "*.import" -not -path "*/.godot/*" -not -path "*/tests/*" | while read -r import; do
  source=$(sed -n 's/^source_file="res:\/\/\(.*\)"$/\1/p' "$import")
  grep -o 'res://[^"]*' <<<"$(grep '^dest_files=' "$import")" | sed 's|^res://||' | while read -r dest; do
    [ -f "$client/$dest" ] && echo "$(size_of "$client/$dest") $source"
  done
done | awk '{ n = split($2, p, "/"); top = (n > 2) ? p[1] "/" p[2] : p[1]; s[top] += $1 }
  END { for (t in s) printf "%10.1f MB  %s\n", s[t] / 1048576, t }' | sort -rn
echo "-- heaviest files in the pack"
godot --headless --path "$client" --export-pack "$preset" "$out/soul-invasion.zip" >"$out/export-zip.log" 2>&1 || true
if [ -s "$out/soul-invasion.zip" ] && command -v unzip >/dev/null; then
  unzip -l "$out/soul-invasion.zip" | awk 'NR > 3 && $1 ~ /^[0-9]+$/ && $4 != "" { printf "%10.1f MB  %s\n", $1 / 1048576, $4 }' | sort -rn | head -6
  echo "pack zipped (what a player downloads of it): $(mb "$(size_of "$out/soul-invasion.zip")")"
fi

echo "== game ($preset)"
version=$(godot --version 2>/dev/null | sed -E 's/^([0-9]+\.[0-9]+(\.[0-9]+)?\.[a-z0-9]+).*/\1/')
templates="${XDG_DATA_HOME:-$HOME/.local/share}/godot/export_templates/$version"
if [ -f "$templates/linux_release.x86_64" ]; then
  godot --headless --path "$client" --export-release "$preset" "$out/soul-invasion.x86_64" >"$out/export-release.log" 2>&1 \
    || { echo "release export failed, see $out/export-release.log"; exit 1; }
  engine=$(size_of "$out/soul-invasion.x86_64")
  echo "soul-invasion.x86_64 (engine): $(mb "$engine")"
  echo "engine and pack: $(mb $((engine + pack)))"
  if command -v zip >/dev/null; then
    rm -f "$out/soul-invasion-linux.zip"
    (cd "$out" && zip -q -9 soul-invasion-linux.zip soul-invasion.x86_64 soul-invasion.pck)
    echo "zipped for download: $(mb "$(size_of "$out/soul-invasion-linux.zip")")"
  fi
else
  echo "no export templates for Godot $version at $templates: the engine's size is not measured"
  echo "(a Linux release template is about 70-90 MB; install them to measure the whole game)"
fi
