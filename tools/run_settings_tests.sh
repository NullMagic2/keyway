#!/usr/bin/env bash
# Module: keyway.tools.run_settings_tests
# Purpose: Test and screenshot the settings window on a private X server, in each language.
# Created: 2026-11-08
# Architecture: The isolated environment is set before dbus-run-session, because everything
# the bus activates inherits it. Screenshots go to .build/screenshots/LANGUAGE/. With
# KEYWAY_CINNAMON_ROOT set to a Cinnamon package root, it tests that desktop's settings instead.
set -euo pipefail

project_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
source "$project_dir/tools/script_common.sh"

readonly TIMEOUT_SECONDS=180
readonly DISPLAY_NUMBER=:97
readonly SCREEN_SIZE=1100x900x24
readonly LANGUAGES=(en_US es_ES pt_BR pt_PT el_GR)
readonly FALLBACK_LOCALE=C.UTF-8
readonly GNOME_DESKTOP=ubuntu:GNOME
readonly CINNAMON_DESKTOP=X-Cinnamon

cargo_cmd="${CARGO:-cargo}"
theme="${KEYWAY_TEST_THEME:-Yaru}"
base_dir="$project_dir/.build/settings-test"
screenshot_dir="${KEYWAY_SCREENSHOT_ROOT:-$project_dir/.build/screenshots}"
cinnamon_root="${KEYWAY_CINNAMON_ROOT:-}"
cinnamon_sandbox=()
desktop="$GNOME_DESKTOP"
layout_schema=org.gnome.desktop.input-sources
layout_key=sources
two_layouts="[('xkb','br'),('xkb','gr+simple')]"

"$cargo_cmd" test --manifest-path "$project_dir/Cargo.toml" --locked \
    --target-dir "$project_dir/.build" -p keyway-settings --test settings_window --no-run

rm -rf -- "$base_dir"
mkdir -p -- "$base_dir"

if [[ -n "$cinnamon_root" ]]; then
    prepare_cinnamon_sandbox "$cinnamon_root" "$base_dir/schemas" "$project_dir/.build"
    export GSETTINGS_SCHEMA_DIR="$base_dir/schemas"
    desktop="$CINNAMON_DESKTOP"

    if cinnamon_uses_input_sources "$cinnamon_root"; then
        layout_schema=org.cinnamon.desktop.input-sources
    else
        layout_schema=org.gnome.libgnomekbd.keyboard
        layout_key=layouts
        two_layouts="['br', 'gr\tsimple']"
    fi
fi

test_binary="$(ls -t "$project_dir"/.build/debug/deps/settings_window-* | grep -v '\.d$' | head -n 1)"

Xvfb "$DISPLAY_NUMBER" -screen 0 "$SCREEN_SIZE" >/dev/null 2>&1 &
xvfb_pid=$!
trap 'kill "$xvfb_pid" 2>/dev/null || true' EXIT
sleep 1

settings_before="$(real_settings_fingerprint)"
status=0

for language in "${LANGUAGES[@]}"; do
    printf '=== %s\n' "$language"

    work_dir="$base_dir/$language"
    rm -rf -- "$work_dir"
    mkdir -p -- "$work_dir/home/.local/share/applications" "$work_dir/run" \
        "$screenshot_dir/$language"
    chmod 700 -- "$work_dir/run"

    # Stands in for the desktop's keyboard panel, so opening it can be observed without it. It
    # records its arguments, and renames the record so that it appears complete.
    panel_marker="$work_dir/panel-opened"
    panel_script="$work_dir/panel-stand-in"
    printf '#!/bin/sh\nprintf "%%s\\n" "$*" > %s.new\nmv %s.new %s\n' \
        "$panel_marker" "$panel_marker" "$panel_marker" > "$panel_script"
    chmod +x -- "$panel_script"
    panel_entry="$(mktemp "$work_dir/panel.XXXXXX")"
    printf '[Desktop Entry]\nType=Application\nName=Panel\nExec=%s\n' "$panel_script" \
        > "$panel_entry"
    for panel_name in gnome-keyboard-panel cinnamon-settings-keyboard; do
        cp -- "$panel_entry" "$work_dir/home/.local/share/applications/$panel_name.desktop"
    done
    rm -f -- "$panel_entry"

    # Stands in for systemctl, which would restart the real services of whoever runs this. It
    # logs its arguments, waits while a file named hold exists, as on a password dialog, and
    # refuses to restart the router while a file named refuse exists.
    systemctl_dir="$work_dir/systemctl"
    mkdir -p -- "$systemctl_dir"
    cat > "$systemctl_dir/systemctl" <<'STAND_IN'
#!/bin/sh
directory="$(dirname "$0")"
echo "$*" >> "$directory/log"
while [ -e "$directory/hold" ]; do sleep 0.05; done
[ ! -e "$directory/refuse" ] || [ "$1" != restart ]
STAND_IN
    chmod +x -- "$systemctl_dir/systemctl"

    # The window chooses its language from LANGUAGE, so an uninstalled locale still works.
    locale="$language.UTF-8"

    if ! "${cinnamon_sandbox[@]}" locale -a 2>/dev/null | grep -qix "$language.utf8"; then
        locale="$FALLBACK_LOCALE"
    fi

    env -u DBUS_SESSION_BUS_ADDRESS -u WAYLAND_DISPLAY \
        HOME="$work_dir/home" \
        XDG_CONFIG_HOME="$work_dir/home/.config" \
        XDG_DATA_HOME="$work_dir/home/.local/share" \
        XDG_CACHE_HOME="$work_dir/home/.cache" \
        XDG_STATE_HOME="$work_dir/home/.local/state" \
        XDG_RUNTIME_DIR="$work_dir/run" \
        GSETTINGS_BACKEND=keyfile \
        DISPLAY="$DISPLAY_NUMBER" \
        GDK_BACKEND=x11 \
        GTK_THEME="$theme" \
        NO_AT_BRIDGE=1 \
        LANGUAGE="$language" \
        LC_ALL="$locale" \
        XDG_CURRENT_DESKTOP="$desktop" \
        KEYWAY_SETTINGS_TEST=1 \
        KEYWAY_PANEL_MARKER="$panel_marker" \
        KEYWAY_SYSTEMCTL_DIR="$systemctl_dir" \
        PATH="$systemctl_dir:$PATH" \
        KEYWAY_SCREENSHOT_DIR="$screenshot_dir/$language" \
        timeout "$TIMEOUT_SECONDS" "${cinnamon_sandbox[@]}" dbus-run-session -- bash -c '
            gsettings set "$1" "$2" "$3"
            exec "$4"
        ' _ "$layout_schema" "$layout_key" "$two_layouts" "$test_binary" \
        || status=$?
done

if [[ "$(real_settings_fingerprint)" != "$settings_before" ]]; then
    printf 'WARNING: real input source or Shell settings changed during this run.\n' >&2
    status=1
fi

exit "$status"
