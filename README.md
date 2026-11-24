<!--
Module: keyway.documentation.readme
Purpose: Welcome a visitor and say what Keyway does.
Created: 2026-11-24
Architecture: ARCHITECTURE.md explains how it works; this file only introduces it.
-->
# Keyway

Keyway is a keyboard language switcher for Linux that works like the one in Windows.

## What it does

- **Switches layouts with a shortcut.** Alt+Shift by default. Any shortcut works, and each
  layout can have one of its own.
- **Types characters by Alt code.** Hold Alt and type a number on the keypad: Alt+130 gives é.
- **Shows the active layout in the taskbar**, with its flag and code, such as PT-BR, and a
  menu to pick another.
- **Has an on-screen keyboard** that shows where the active layout puts each character, and
  types the keys you click.
- **Has a settings window** for layouts, shortcuts and Alt codes, in English, Portuguese and
  Greek.

It runs on GNOME Shell 46 or later and Cinnamon 6.2 or later.

## Install

Download the package for Ubuntu, Debian or Linux Mint from the
[latest release](https://github.com/NullMagic2/keyway/releases/latest), then install it:

```bash
sudo apt install ./keyway_*.deb
```

Keyway starts at your next login.

To build the packages yourself, run `tools/build_packages.sh`. It needs Rust 1.88 or later and
the GTK 3 and xkbcommon development files.

## Learn more

[ARCHITECTURE.md](ARCHITECTURE.md) explains how Keyway works. It is released under the
[MIT license](LICENSE).
