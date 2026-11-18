#!/usr/bin/env bash
set -e
cd "$(dirname "$0")"

# Build once if the release binary is missing.
if [ ! -x target/release/gta7-rust ]; then
  echo "Building gta7-rust (release)..."
  cargo build --release
fi

# Defaults for this machine; override by exporting before running the script.
export DISPLAY="${DISPLAY:-:0}"
export WGPU_BACKEND="${WGPU_BACKEND:-vulkan}"
export VK_ICD_FILENAMES="${VK_ICD_FILENAMES:-/usr/share/vulkan/icd.d/lvp_icd.json}"

exec ./target/release/gta7-rust
