#!/usr/bin/env python3
# Module: keyway.tools.package_vm_scenario
# Purpose: Inside a throwaway VM, install the packages and test the router under real systemd.
# Created: 2026-11-09
# Architecture: Booted by run_package_vm_tests.sh as a system service. It stands in for the agent,
# speaking the router's protocol over its socket, because no desktop runs here. /dev/uinput starts
# as a uaccess tag leaves it, with an access list that gives its group nothing.
import glob
import json
import os
import socket
import subprocess
import sys
import time

ROUTER_UNIT = "keyway-router.service"
ROUTER_SOCKET = "/run/keyway/router.sock"
UINPUT_PATH = "/dev/uinput"
PACKAGE_DIRECTORY = "/keyway-vm/packages"
TEST_USER = "tester"
TEST_UID = 1000
OTHER_UID = 65534
PROTOCOL_VERSION = 1
PASS_MARKER = "KEYWAY_PACKAGE_VM_TESTS_PASSED"
ROUTER_RETRIES_SECONDS = 8
SHORT_WAIT_SECONDS = 15
ANSWER_SECONDS = 5
AUTOLOGIN_DROP_IN = "/etc/systemd/system/getty@tty1.service.d/autologin.conf"

failures = 0


def run(*command):
    return subprocess.run(command, capture_output=True, text=True).stdout.strip()


def check(name, passed, detail=""):
    global failures

    print(f"{'ok  ' if passed else 'FAIL'} {name}{f': {detail}' if detail else ''}", flush=True)

    if not passed:
        failures += 1


def wait_until(condition, seconds=SHORT_WAIT_SECONDS):
    deadline = time.monotonic() + seconds

    while time.monotonic() < deadline:
        if condition():
            return True

        time.sleep(0.2)

    return condition()


def router_log():
    return run("journalctl", "-u", ROUTER_UNIT, "--no-pager", "-o", "cat")


def access_list():
    return run("getfacl", "-cpn", UINPUT_PATH).splitlines()


def active_session_uid():
    session = run("loginctl", "show-seat", "seat0", "--property=ActiveSession", "--value")

    return run("loginctl", "show-session", session, "--property=User", "--value") if session else ""


def leave_uinput_as_a_uaccess_tag_does():
    run("setfacl", "-b", UINPUT_PATH)
    run("chgrp", "root", UINPUT_PATH)
    run("chmod", "600", UINPUT_PATH)
    run("setfacl", "-m", f"user:{TEST_UID}:rw", UINPUT_PATH)
    check("uinput starts with no group access", "group::---" in access_list(), str(access_list()))


def failed_starts():
    return router_log().count("Failed with result")


def install_packages():
    """Returns how many router starts had failed before, such as at boot for an old package that
    the test root still had installed."""
    failed_before = failed_starts()

    for package in sorted(glob.glob(f"{PACKAGE_DIRECTORY}/*.deb")):
        result = subprocess.run(["dpkg", "-i", package], capture_output=True, text=True)
        check(f"dpkg installs {os.path.basename(package)}", result.returncode == 0,
              result.stderr.strip())

    # Long enough for a router that keeps failing to use up all its restarts.
    time.sleep(ROUTER_RETRIES_SECONDS)

    return failed_before


def test_router_starts(failed_before):
    state = run("systemctl", "is-active", ROUTER_UNIT)
    check("router is running", state == "active", state)
    check("without restarting", run("systemctl", "show", ROUTER_UNIT, "--property=NRestarts",
                                    "--value") == "0")

    # The installer's own restart sets the counter above back to zero, so read the log too.
    check("no start failed", failed_starts() == failed_before, router_log())
    check("input group can use uinput", "group::rw-" in access_list(), str(access_list()))
    check("uaccess user keeps its access", f"user:{TEST_UID}:rw-" in access_list())
    check("node belongs to input", run("stat", "-c", "%G", UINPUT_PATH) == "input")
    check("socket exists", os.path.exists(ROUTER_SOCKET))

    with open("/proc/bus/input/devices", encoding="utf-8") as devices:
        check("virtual keyboard exists", "Keyway virtual keyboard" in devices.read())


def log_in_test_user():
    os.makedirs(os.path.dirname(AUTOLOGIN_DROP_IN), exist_ok=True)

    with open(AUTOLOGIN_DROP_IN, "w", encoding="utf-8") as drop_in:
        drop_in.write(f"[Service]\nExecStart=\nExecStart=-/sbin/agetty --autologin {TEST_USER} "
                      "--noclear %I $TERM\n")

    run("systemctl", "daemon-reload")
    run("systemctl", "restart", "getty@tty1.service")

    check("logind has an active session", wait_until(lambda: active_session_uid() == str(TEST_UID)),
          active_session_uid())


def start_agent(uid, stays):
    """Runs this file as the stand-in agent. It prints the router's first answer."""
    return subprocess.Popen([sys.executable, "-I", __file__, "agent", str(stays)], user=uid,
                            stdout=subprocess.PIPE, text=True)


def test_session_check():
    other = start_agent(OTHER_UID, False)
    answer = other.communicate(timeout=SHORT_WAIT_SECONDS)[0].strip()
    check("router refuses a user without the active session", answer == "closed", answer)

    agent = start_agent(TEST_UID, True)
    answer = agent.stdout.readline().strip()
    check("router accepts the active session's user", answer == "hello", answer)
    check("router grabs the keyboard in its sandbox", wait_until(lambda: "grabbed" in router_log()),
          router_log())

    agent.kill()
    agent.wait()
    check("router releases it when the agent goes",
          wait_until(lambda: "keyboards released" in router_log()))
    check("router is still running", run("systemctl", "is-active", ROUTER_UNIT) == "active")


def be_the_agent(stays):
    configure = {"configure": {
        "shortcuts": [],
        "alt_codes": {"enabled": True, "numlock": "ignored", "hex_numpad": False,
                      "allow_right_alt": False},
        "excluded_devices": [],
    }}

    connection = socket.socket(socket.AF_UNIX)
    connection.settimeout(ANSWER_SECONDS)
    connection.connect(ROUTER_SOCKET)
    connection.sendall((json.dumps({"hello": {"version": PROTOCOL_VERSION}}) + "\n").encode())

    try:
        answer = connection.recv(4096)
    except (OSError, TimeoutError):
        answer = b""

    print("hello" if b"hello" in answer else "closed", flush=True)

    if stays and answer:
        connection.sendall((json.dumps(configure) + "\n").encode())
        time.sleep(ROUTER_RETRIES_SECONDS * SHORT_WAIT_SECONDS)


def main():
    run("dpkg", "--purge", "keyway")
    run("useradd", "-m", "-u", str(TEST_UID), TEST_USER)

    leave_uinput_as_a_uaccess_tag_does()
    failed_before = install_packages()
    test_router_starts(failed_before)

    if failures == 0:
        log_in_test_user()
        test_session_check()
    else:
        print(router_log(), flush=True)

    print(f"{failures} failed", flush=True)

    if failures == 0:
        print(PASS_MARKER, flush=True)


if __name__ == "__main__":
    if sys.argv[1:2] == ["agent"]:
        be_the_agent(sys.argv[2] == "True")
    else:
        main()
