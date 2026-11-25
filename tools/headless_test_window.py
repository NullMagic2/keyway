"""
Module: keyway.tools.headless_test_window
Purpose: A focused GTK window reporting what it receives, for the headless agent test.
Created: 2026-11-08
Architecture: Mode entry accepts text input; mode keys has none, like many older clients.
Mode overlay covers the screen and stays above other windows, like a screenshot tool.
"""

import sys

import gi

gi.require_version("Gdk", "3.0")
gi.require_version("Gtk", "3.0")
from gi.repository import Gdk, Gtk


def report(line):
    print(line, flush=True)


def report_key(_widget, event):
    report(f"KEY {Gdk.keyval_name(event.keyval)}")
    return True


def main():
    mode = sys.argv[1]
    window = Gtk.Window(title=f"keyway-test-{mode}")
    window.set_default_size(400, 120)

    if mode == "entry":
        entry = Gtk.Entry()
        entry.connect("changed", lambda widget: report(f"TEXT {widget.get_text()}"))
        window.add(entry)
    elif mode == "overlay":
        window.set_keep_above(True)
        window.fullscreen()
        window.add_events(Gdk.EventMask.BUTTON_PRESS_MASK)
        window.connect("button-press-event", lambda *_: report("CLICK"))
    else:
        window.connect("key-press-event", report_key)

    window.connect("destroy", Gtk.main_quit)
    window.show_all()
    window.present()
    Gtk.main()


if __name__ == "__main__":
    main()
