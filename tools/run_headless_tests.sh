#!/usr/bin/env bash
# Module: keyway.tools.run_headless_tests
# Purpose: Test the agent and extension against a real GNOME Shell, fully isolated.
# Created: 2026-11-08
# Architecture: The isolated environment must be set before dbus-run-session, because
# everything the bus activates inherits it. Otherwise dconf writes reach the real settings.
set -euo pipefail

project_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
source "$project_dir/tools/script_common.sh"

readonly TIMEOUT_SECONDS=120

cargo_cmd="${CARGO:-cargo}"
work_dir="$project_dir/.build/headless"

"$cargo_cmd" build --manifest-path "$project_dir/Cargo.toml" --locked \
    --target-dir "$project_dir/.build" -p keyway -p keyway-settings

rm -rf -- "$work_dir"
mkdir -p -- "$work_dir/home"
link_system_data_without_keyway "$work_dir/system-data"

# Wayland socket paths are limited to 108 bytes, so this cannot live under the project.
runtime_dir="$(mktemp -d "/run/user/$(id -u)/keyway-test.XXXXXX")"
chmod 700 -- "$runtime_dir"
# Services the test bus started can outlive it briefly and still write here.
remove_runtime_dir() {
    for _attempt in 1 2 3 4 5; do
        rm -rf -- "$runtime_dir" 2>/dev/null && return 0
        sleep 1
    done

    printf 'WARNING: could not remove %s\n' "$runtime_dir" >&2
}
trap remove_runtime_dir EXIT

settings_before="$(real_settings_fingerprint)"
status=0

env -u DBUS_SESSION_BUS_ADDRESS -u DISPLAY -u WAYLAND_DISPLAY \
    HOME="$work_dir/home" \
    XDG_CONFIG_HOME="$work_dir/home/.config" \
    XDG_DATA_HOME="$work_dir/home/.local/share" \
    XDG_CACHE_HOME="$work_dir/home/.cache" \
    XDG_STATE_HOME="$work_dir/home/.local/state" \
    XDG_DATA_DIRS="$work_dir/system-data" \
    XDG_RUNTIME_DIR="$runtime_dir" \
    GSETTINGS_BACKEND=keyfile \
    LIBGL_ALWAYS_SOFTWARE=1 \
    GDK_BACKEND=wayland \
    GTK_THEME=Adwaita \
    NO_AT_BRIDGE=1 \
    LANGUAGE=en_US \
    LC_ALL=C.UTF-8 \
    timeout "$TIMEOUT_SECONDS" dbus-run-session -- \
    python3 -I "$project_dir/tools/headless_agent_test.py" gnome \
    "$project_dir/.build/debug/keyway" "$project_dir/tools/headless_test_window.py" "$work_dir" \
    || status=$?

if [[ "$(real_settings_fingerprint)" != "$settings_before" ]]; then
    printf 'WARNING: real input source or Shell settings changed during this run.\n' >&2
    status=1
fi

exit "$status"
