#!/usr/bin/env bash
# Module: keyway.tools.build_packages
# Purpose: Build the Ubuntu, Debian and Linux Mint .deb packages into packages/.
# Created: 2026-11-08
# Architecture: Packages the binaries from build.sh with the files in packaging/. MAINTAINER
# overrides the Maintainer field. Installs nothing, and needs no root.
set -euo pipefail

# Package files must not inherit a group-writable mode from the builder's environment.
umask 022

project_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
source "$project_dir/tools/script_common.sh"

readonly PACKAGE_NAME=keyway
readonly DEBIAN_REVISION=1
readonly ARCHITECTURE=amd64
readonly MINIMUM_SHELL=46
readonly MINIMUM_CINNAMON=6.2
readonly DEFAULT_MAINTAINER='Keyway maintainers <28734029+NullMagic2@users.noreply.github.com>'
readonly GNOME_DEPENDENCY="gnome-shell (>= $MINIMUM_SHELL)"
readonly CINNAMON_DEPENDENCY="cinnamon (>= $MINIMUM_CINNAMON)"
readonly SESSION_ENTRY_PATH=/etc/xdg/autostart/keyway.desktop
readonly RUNTIME_DEPENDENCIES='acl, dbus-user-session, gsettings-desktop-schemas, init-system-helpers, librsvg2-common, libsystemd0, systemd, udev, x11-xkb-utils, xkb-data'
readonly ROUTER_BINARY_PATH=/usr/libexec/keyway-router
readonly AGENT_BINARY_PATH=/usr/bin/keyway
readonly SETTINGS_BINARY_PATH=/usr/bin/keyway-settings

maintainer="${MAINTAINER:-$DEFAULT_MAINTAINER}"
package_dir="$project_dir/packages"
work_dir="$project_dir/.build/packages"
version="$(sed -n 's/^version = "\(.*\)"$/\1/p' "$project_dir/Cargo.toml" | head -n 1)"

if [[ -z "$version" ]]; then
    fail 'no version found in Cargo.toml'
fi

"$project_dir/build.sh"

rm -rf -- "$work_dir"
mkdir -p -- "$work_dir" "$package_dir"

stripped_dir="$work_dir/stripped"
mkdir -p -- "$stripped_dir/debian"

for binary_name in "${BINARY_NAMES[@]}"; do
    install -m 755 -- "$project_dir/bin/$binary_name" "$stripped_dir/$binary_name"
    strip --strip-unneeded -- "$stripped_dir/$binary_name"
done

# dpkg-shlibdeps answers for this machine; newer libraries still provide their pre-t64
# names, and using those keeps the packages installable on releases from before that change.
library_dependencies() {
    local raw

    printf 'Source: %s\n' "$PACKAGE_NAME" > "$stripped_dir/debian/control"
    raw="$(cd -- "$stripped_dir" && dpkg-shlibdeps -O "${BINARY_NAMES[@]}" 2>/dev/null)"
    raw="${raw#shlibs:Depends=}"

    printf '%s' "$raw" | sed -E \
        -e 's/libglib2\.0-0t64/libglib2.0-0/' \
        -e 's/libgtk-3-0t64/libgtk-3-0/'
}

shared_libraries="$(library_dependencies)"

# Lists the crates linked into the binaries, with their licenses.
third_party_notices() {
    cargo metadata --manifest-path "$project_dir/Cargo.toml" --locked --offline \
        --format-version 1 2>/dev/null | python3 -I -c '
import json, sys

metadata = json.load(sys.stdin)
members = set(metadata["workspace_members"])
packages = [p for p in metadata["packages"] if p["id"] not in members]

for package in sorted(packages, key=lambda p: (p["name"], p["version"])):
    print(f" {package['"'"'name'"'"']} {package['"'"'version'"'"']}: {package['"'"'license'"'"'] or '"'"'see crate'"'"'}")
'
}

write_tree() {
    local root="$1"

    install -D -m 755 -- "$stripped_dir/keyway" "$root$AGENT_BINARY_PATH"
    install -D -m 755 -- "$stripped_dir/keyway-settings" "$root$SETTINGS_BINARY_PATH"
    install -D -m 755 -- "$stripped_dir/keyway-router" "$root$ROUTER_BINARY_PATH"

    install -D -m 644 -- "$project_dir/packaging/70-keyway.rules" \
        "$root/usr/lib/udev/rules.d/70-keyway.rules"
    install -D -m 644 -- "$project_dir/packaging/30-keyway.quirks" \
        "$root/usr/share/libinput/30-keyway.quirks"
    install -D -m 644 -- "$project_dir/data/config.example.toml" \
        "$root/usr/share/doc/$PACKAGE_NAME/config.example.toml"

    install -d -- "$root/usr/lib/systemd/system" "$root/usr/lib/systemd/user" \
        "$root/usr/share/applications"
    sed "s#@KEYWAY_ROUTER_BIN@#$ROUTER_BINARY_PATH#" "$project_dir/packaging/keyway-router.service" \
        > "$root/usr/lib/systemd/system/keyway-router.service"
    sed "s#@KEYWAY_BIN@#$AGENT_BINARY_PATH#" "$project_dir/packaging/keyway.service" \
        > "$root/usr/lib/systemd/user/keyway.service"
    sed "s#@KEYWAY_SETTINGS_BIN@#$SETTINGS_BINARY_PATH#" "$project_dir/packaging/keyway-settings.desktop" \
        > "$root/usr/share/applications/keyway-settings.desktop"

    install -d -- "$root$(dirname -- "$SESSION_ENTRY_PATH")"
    sed "s#@KEYWAY_BIN@#$AGENT_BINARY_PATH#" "$project_dir/packaging/keyway-session.desktop" \
        > "$root$SESSION_ENTRY_PATH"

    local desktop
    local extension_file

    for desktop in gnome-shell:gnome cinnamon:cinnamon; do
        for extension_file in "$project_dir/extension/${desktop#*:}/keyway@local"/*; do
            install -D -m 644 -t "$root/usr/share/${desktop%:*}/extensions/keyway@local" \
                -- "$extension_file"
        done
    done

    # keyway setup reads this to leave the package's files to the package manager.
    install -d -- "$root/usr/share/$PACKAGE_NAME"
    : > "$root/usr/share/$PACKAGE_NAME/packaged"

    # The extensions read the taskbar's layout labels and flags from here.
    install -D -m 644 -- "$project_dir/data/layout_labels.json" \
        "$root/usr/share/$PACKAGE_NAME/layout_labels.json"
    install -d -- "$root/usr/share/$PACKAGE_NAME/flags"
    install -m 644 -- "$project_dir"/data/flags/*.svg "$root/usr/share/$PACKAGE_NAME/flags/"

    local doc_dir="$root/usr/share/doc/$PACKAGE_NAME"
    {
        printf 'Format: https://www.debian.org/doc/packaging-manuals/copyright-format/1.0/\n'
        printf 'Upstream-Name: Keyway\n\nFiles: *\nCopyright: 2026 NMagic\nLicense: MIT\n'
        sed -e 's/^$/./' -e 's/^/ /' -- "$project_dir/LICENSE"
        printf '\nFiles: usr/share/keyway/flags/*\nCopyright: 2013 Panayiotis Lipiridis\n'
        printf 'License: MIT\n'
        sed -e 's/^$/./' -e 's/^/ /' -- "$project_dir/data/flags/LICENSE"
        printf '\nComment: Third-party crates linked into the binaries:\n'
        third_party_notices
    } > "$doc_dir/copyright"

    printf '%s (%s) stable; urgency=medium\n\n  * Keyway is under the MIT license, and the package now says so.\n\n -- %s  %s\n' \
        "$PACKAGE_NAME" "$2" "$maintainer" "$(date -R)" \
        | gzip -9n > "$doc_dir/changelog.gz"
}

# Arguments: distribution tag, how it relates to a desktop, and a line for the description.
build_package() {
    local distribution="$1"
    local desktop_relation="$2"
    local description_note="$3"
    local full_version="$version-$DEBIAN_REVISION~${distribution}1"
    local root="$work_dir/$distribution"
    local depends="$shared_libraries, $RUNTIME_DEPENDENCIES"
    local output="$package_dir/${PACKAGE_NAME}_${full_version}_${ARCHITECTURE}.deb"

    write_tree "$root" "$full_version"

    if [[ "$desktop_relation" == depends ]]; then
        depends="$depends, $GNOME_DEPENDENCY | $CINNAMON_DEPENDENCY"
    fi

    install -d -- "$root/DEBIAN"

    for script in postinst prerm postrm; do
        install -m 755 -- "$project_dir/packaging/debian/$script" "$root/DEBIAN/$script"
    done

    printf '%s\n' "$SESSION_ENTRY_PATH" > "$root/DEBIAN/conffiles"

    {
        printf 'Package: %s\nVersion: %s\nArchitecture: %s\n' "$PACKAGE_NAME" "$full_version" "$ARCHITECTURE"
        printf 'Maintainer: %s\nSection: utils\nPriority: optional\n' "$maintainer"
        printf 'Installed-Size: %s\nDepends: %s\n' "$(du -sk --exclude=DEBIAN "$root" | cut -f1)" "$depends"

        # Suggests, because apt installs Recommends by default, and a Mint edition with another
        # desktop should not have a whole second desktop pulled in.
        if [[ "$desktop_relation" == suggests ]]; then
            printf 'Suggests: %s, %s\n' "$CINNAMON_DEPENDENCY" "$GNOME_DEPENDENCY"
        fi

        printf 'Description: Windows-style keyboard language switcher\n'
        printf ' Replaces the desktop keyboard layout switcher at the next login. Switches\n'
        printf ' layouts with shortcuts such as Alt+Shift and types characters with Alt plus\n'
        printf ' keypad codes, with a settings window for layouts and shortcuts.\n .\n %s\n' \
            "$description_note"
    } > "$root/DEBIAN/control"

    (cd -- "$root" && find usr -type f -exec md5sum {} +) > "$root/DEBIAN/md5sums"

    # xz is readable by every supported release's dpkg, unlike a newer default.
    dpkg-deb --root-owner-group -Zxz --build "$root" "$output" >/dev/null
    printf 'Built: %s\n' "$output"
}

build_package ubuntu depends 'Built for Ubuntu 24.04 and later with GNOME Shell or Cinnamon.'
build_package debian depends 'Built for Debian 13 and later with GNOME Shell or Cinnamon.'
build_package mint suggests 'Built for the Cinnamon edition of Linux Mint 22 and later.'
