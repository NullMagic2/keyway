#!/usr/bin/env bash
# Module: keyway.build
# Purpose: Build the workspace and publish its binaries to bin/.
# Created: 2026-11-08
# Architecture: Intermediates stay in .build/; a failed build leaves bin/ untouched.
set -euo pipefail

project_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
source "$project_dir/tools/script_common.sh"

cargo_cmd="${CARGO:-cargo}"
build_dir="$project_dir/.build"
bin_dir="$project_dir/bin"

require_project_root
acquire_project_lock

if [[ ! -f "$project_dir/Cargo.toml" ]]; then
    fail 'no Cargo.toml yet: the workspace has not been written'
fi

"$cargo_cmd" build --manifest-path "$project_dir/Cargo.toml" \
    --locked --release --target-dir "$build_dir"

if ((${#BINARY_NAMES[@]} == 0)); then
    printf 'Built the libraries; no binary crates exist yet, so bin/ is unchanged.\n'
    exit 0
fi

mkdir -p -- "$bin_dir"

# Publish each binary atomically so a running service never sees a partial file.
for binary_name in "${BINARY_NAMES[@]}"; do
    install -m 755 -- "$build_dir/release/$binary_name" "$bin_dir/.$binary_name.new"
    mv -f -- "$bin_dir/.$binary_name.new" "$bin_dir/$binary_name"
    printf 'Built: %s\n' "$bin_dir/$binary_name"
done
