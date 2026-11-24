#!/usr/bin/env bash
# Module: keyway.tools.run_cinnamon_tests
# Purpose: Test the agent and extension against a real Cinnamon, fully isolated.
# Created: 2026-11-08
# Architecture: Runs Cinnamon in a sandbox from KEYWAY_CINNAMON_ROOT, either unpacked packages or
# a complete system such as Linux Mint, so nothing is installed here. Its fresh runtime directory
# keeps socket paths short enough. The isolated environment is set before dbus-run-session,
# because everything the bus activates inherits it. KEYWAY_TEST_BINARY tests another keyway,
# such as the /usr/bin/keyway a package put inside that system.
set -euo pipefail

project_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
source "$project_dir/tools/script_common.sh"

readonly TIMEOUT_SECONDS=180

cargo_cmd="${CARGO:-cargo}"
work_dir="$project_dir/.build/cinnamon"
cinnamon_root="${KEYWAY_CINNAMON_ROOT:-}"
cinnamon_sandbox=()

if [[ -z "$cinnamon_root" ]]; then
    fail 'set KEYWAY_CINNAMON_ROOT to an unpacked Cinnamon and its dependencies'
fi

"$cargo_cmd" build --manifest-path "$project_dir/Cargo.toml" --locked \
    --target-dir "$project_dir/.build" -p keyway -p keyway-settings

rm -rf -- "$work_dir"
mkdir -p -- "$work_dir/home"

prepare_cinnamon_sandbox "$cinnamon_root" "$work_dir/schemas" "$project_dir/.build"

settings_before="$(real_settings_fingerprint)"
status=0

env -i \
    PATH="$PATH" \
    HOME="$work_dir/home" \
    XDG_CONFIG_HOME="$work_dir/home/.config" \
    XDG_DATA_HOME="$work_dir/home/.local/share" \
    XDG_CACHE_HOME="$work_dir/home/.cache" \
    XDG_STATE_HOME="$work_dir/home/.local/state" \
    XDG_RUNTIME_DIR="/run/user/$(id -u)" \
    XDG_CURRENT_DESKTOP=X-Cinnamon \
    XDG_SESSION_TYPE=x11 \
    GSETTINGS_BACKEND=keyfile \
    GSETTINGS_SCHEMA_DIR="$work_dir/schemas" \
    GIO_USE_VFS=local \
    GVFS_DISABLE_FUSE=1 \
    LIBGL_ALWAYS_SOFTWARE=1 \
    GDK_BACKEND=x11 \
    GTK_THEME=Adwaita \
    NO_AT_BRIDGE=1 \
    LANGUAGE=en_US \
    LC_ALL=C.UTF-8 \
    KEYWAY_SCREENSHOT_DIR="${KEYWAY_SCREENSHOT_DIR:-}" \
    timeout "$TIMEOUT_SECONDS" "${cinnamon_sandbox[@]}" dbus-run-session -- \
    python3 -I "$project_dir/tools/headless_agent_test.py" cinnamon \
    "${KEYWAY_TEST_BINARY:-$project_dir/.build/debug/keyway}" \
    "$project_dir/tools/headless_test_window.py" "$work_dir" \
    || status=$?

if [[ "$(real_settings_fingerprint)" != "$settings_before" ]]; then
    printf 'WARNING: real input source or desktop settings changed during this run.\n' >&2
    status=1
fi

exit "$status"
