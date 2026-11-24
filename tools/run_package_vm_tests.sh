#!/usr/bin/env bash
# Module: keyway.tools.run_package_vm_tests
# Purpose: Install real packages and test the router under real systemd, in a throwaway VM.
# Created: 2026-11-09
# Architecture: Boots a complete system such as Linux Mint from a copy, with no network. A small
# first stage gives its files to root, since they were unpacked without root, then hands over to
# systemd. Packages install in the given order, so an old one then a new one tests the upgrade.
# Usage: tools/run_package_vm_tests.sh PATH_TO_VMLINUZ SYSTEM_ROOT PACKAGE.deb...
set -euo pipefail

project_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
source "$project_dir/tools/script_common.sh"

readonly PASS_MARKER='KEYWAY_PACKAGE_VM_TESTS_PASSED'
readonly VM_MEMORY_MB=2048
readonly VM_TIMEOUT_SECONDS=300
readonly DISK_SIZE=6G
readonly PAYLOAD_PATH=/keyway-vm
readonly TEST_UNIT=keyway-vm-test.service

kernel_path="${1:-}"
system_root="${2:-}"

if [[ $# -lt 3 ]]; then
    fail 'usage: tools/run_package_vm_tests.sh PATH_TO_VMLINUZ SYSTEM_ROOT PACKAGE.deb...'
fi

shift 2

work_dir="$project_dir/.build/package-vm"
initrd_dir="$work_dir/initrd"
payload_dir="$initrd_dir/payload"
disk_path="$work_dir/disk.img"
log_path="$work_dir/vm.log"

rm -rf -- "$work_dir"
mkdir -p -- "$initrd_dir"/{bin,dev,proc,sys,new_root} "$payload_dir/packages"

# The fixed-width number keeps the order given, whatever the packages are called.
package_number=0

for package in "$@"; do
    package_number=$((package_number + 1))
    cp -- "$package" "$payload_dir/packages/$(printf '%02d' "$package_number").deb"
done

cp -- "$project_dir/tools/package_vm_scenario.py" "$payload_dir/scenario.py"
cp -- /usr/bin/busybox "$initrd_dir/bin/busybox"

cat > "$payload_dir/$TEST_UNIT" <<EOF
[Unit]
Description=Keyway package tests
After=basic.target dbus.service systemd-logind.service

[Service]
Type=oneshot
ExecStart=/usr/bin/python3 -I $PAYLOAD_PATH/scenario.py
ExecStopPost=/usr/bin/systemctl poweroff --force --force
StandardOutput=tty
StandardError=tty
TTYPath=/dev/ttyS0
EOF

cat > "$initrd_dir/init" <<EOF
#!/bin/busybox sh
/bin/busybox --install -s /bin
mount -t proc proc /proc
mount -t sysfs sysfs /sys
mount -t devtmpfs devtmpfs /dev
mount -t ext4 -o rw /dev/vda /new_root
chown -hR 0:0 /new_root
cp -r /payload /new_root$PAYLOAD_PATH
wants=/new_root/etc/systemd/system/multi-user.target.wants
mkdir -p "\$wants"
ln -s $PAYLOAD_PATH/$TEST_UNIT /new_root/etc/systemd/system/$TEST_UNIT
ln -s ../$TEST_UNIT "\$wants/$TEST_UNIT"
umount /proc /sys /dev
exec switch_root /new_root /lib/systemd/systemd
EOF
chmod +x -- "$initrd_dir/init"

(cd -- "$initrd_dir" && find . | cpio --quiet -o -H newc | gzip -1) > "$work_dir/initrd.cpio.gz"

# Inside a user namespace, because an unpacked system can hold directories of its own users.
unshare --map-auto --map-root-user \
    /usr/sbin/mke2fs -q -F -t ext4 -d "$system_root" "$disk_path" "$DISK_SIZE"

status=0

timeout "$VM_TIMEOUT_SECONDS" qemu-system-x86_64 \
    -enable-kvm -cpu host -m "$VM_MEMORY_MB" \
    -kernel "$kernel_path" -initrd "$work_dir/initrd.cpio.gz" \
    -drive "file=$disk_path,format=raw,if=virtio" \
    -append 'console=ttyS0 quiet panic=-1 systemd.unit=multi-user.target systemd.mask=serial-getty@ttyS0.service' \
    -nographic -no-reboot -nic none \
    | tee "$log_path" || status=$?

rm -f -- "$disk_path"

if [[ "$status" -ne 0 ]] || ! grep -q -- "$PASS_MARKER" "$log_path"; then
    exit 1
fi
