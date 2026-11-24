# Module: keyway.tools.script_common
# Purpose: Share project checks, the binary list and the lock between scripts.
# Created: 2026-11-08
# Architecture: Sourced by build.sh and clean_source.sh after they set project_dir.

readonly BINARY_NAMES=(keyway keyway-router keyway-settings)
readonly PROJECT_MARKER='Module: keyway.documentation.architecture'
readonly LOCK_FD=9

fail() {
    printf 'Error: %s\n' "$1" >&2
    exit 1
}

# Fingerprints the real settings that desktop tests could touch. Only these, because the
# whole database also changes with ordinary desktop use, such as using a file dialog.
real_settings_fingerprint() {
    dconf dump /org/gnome/desktop/input-sources/ | sha256sum
    dconf dump /org/gnome/desktop/wm/keybindings/ | sha256sum
    dconf dump /org/gnome/shell/ | sha256sum
    dconf dump /org/gnome/libgnomekbd/ | sha256sum
    dconf dump /org/cinnamon/ | sha256sum
}

# Fills a directory with links to everything in /usr/share except Keyway's own data, for use as
# XDG_DATA_DIRS. A desktop under test then cannot see an installed package's marker or labels,
# so the build under test supplies them through keyway setup.
link_system_data_without_keyway() {
    local target="$1"
    local entry

    mkdir -p -- "$target"

    for entry in /usr/share/*; do
        if [[ "${entry##*/}" != keyway ]]; then
            ln -s -- "$entry" "$target/"
        fi
    done
}

# True for a root with a whole system installed, such as Linux Mint, rather than unpacked
# packages to lay over this system. Only an installed system has a package database.
is_complete_system() {
    [[ -e "$1/var/lib/dpkg/status" ]]
}

# True when the root's Cinnamon keeps its layouts as input sources, as 6.6 and later do.
cinnamon_uses_input_sources() {
    [[ -e "$1/usr/share/cinnamon/js/ui/keyboardManager.js" ]]
}

# Fills cinnamon_sandbox with the bwrap arguments for running Cinnamon out of a package root,
# and compiles its schemas into the given directory. A complete system runs alone, on its own
# libraries, and any changes stay in memory. An unpacked root is laid over this system. Either
# way the real home, session bus and system bus stay out of reach, because Cinnamon tries to
# take over the login session.
# Arguments: the root, the schema directory, and the writable work directory.
prepare_cinnamon_sandbox() {
    local package_root="$1"
    local schema_dir="$2"
    local writable_dir="$3"

    mkdir -p -- "$schema_dir"

    if is_complete_system "$package_root"; then
        cp "$package_root/usr/share/glib-2.0/schemas/"* "$schema_dir/"
        cinnamon_sandbox=(
            bwrap --unshare-pid --overlay-src "$package_root" --tmp-overlay /
            --dev /dev --proc /proc --tmpfs /tmp --bind-try /tmp/.X11-unix /tmp/.X11-unix
            --ro-bind /etc/passwd /etc/passwd --ro-bind /etc/group /etc/group
        )
    else
        cp /usr/share/glib-2.0/schemas/* "$package_root/usr/share/glib-2.0/schemas/"* "$schema_dir/"
        cinnamon_sandbox=(
            bwrap --dev-bind / /
            --overlay-src /usr --overlay-src "$package_root/usr" --tmp-overlay /usr
            --overlay-src /etc --overlay-src "$package_root/etc" --tmp-overlay /etc
            --tmpfs "$HOME" --tmpfs /run/dbus --tmpfs /run/systemd
        )
    fi

    glib-compile-schemas "$schema_dir"

    # The empty /usr/share/keyway hides an installed Keyway package, in the root or on this
    # system, which would otherwise stand in for the build under test.
    cinnamon_sandbox+=(
        --tmpfs /usr/share/keyway
        --perms 0700 --tmpfs "/run/user/$(id -u)"
        --ro-bind "$project_dir" "$project_dir"
        --bind "$writable_dir" "$writable_dir"
        --die-with-parent
    )
}

# Deleting by a wrong root would be catastrophic, so identify it by content and path.
require_project_root() {
    if [[ "$project_dir" != "$(cd -- "$project_dir" && pwd -P)" ]]; then
        fail "project path goes through a symlink: $project_dir"
    fi

    if ! grep -qF -- "$PROJECT_MARKER" "$project_dir/ARCHITECTURE.md" 2>/dev/null; then
        fail "not a Keyway project root: $project_dir"
    fi
}

# Locking the directory itself leaves no lock file behind in the tree.
acquire_project_lock() {
    eval "exec $LOCK_FD<\"\$project_dir\""

    if ! flock --nonblock "$LOCK_FD"; then
        fail 'another build or cleanup is running'
    fi
}
