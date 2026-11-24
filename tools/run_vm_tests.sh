#!/usr/bin/env bash
# Module: keyway.tools.run_vm_tests
# Purpose: Run the router's device tests in a throwaway VM with no network or disk.
# Created: 2026-11-08
# Architecture: Virtual keyboards exist system-wide, so on the host these tests would
# type into the desktop. Usage: tools/run_vm_tests.sh PATH_TO_VMLINUZ
set -euo pipefail

project_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"

readonly TARGET='x86_64-unknown-linux-musl'
readonly PASS_MARKER='KEYWAY_VM_TESTS_PASSED'
readonly VM_MEMORY_MB=512
readonly VM_TIMEOUT_SECONDS=120

kernel_path="${1:?usage: tools/run_vm_tests.sh PATH_TO_VMLINUZ}"
build_dir="$project_dir/.build/vm"
root_dir="$build_dir/root"
initrd_path="$build_dir/initrd.cpio.gz"
log_path="$build_dir/vm.log"

# A static build keeps the VM free of any library from the host. The musl standard
# library belongs to the rustup toolchain, so its compiler is used explicitly.
export RUSTC="${RUSTC:-$(rustup which rustc)}"
cargo_cmd="${CARGO:-$HOME/.cargo/bin/cargo}"

build_arguments=(--manifest-path "$project_dir/Cargo.toml" --locked --target "$TARGET"
    --target-dir "$build_dir/target" -p keyway-router)

"$cargo_cmd" build "${build_arguments[@]}" --bin keyway-router
"$cargo_cmd" test "${build_arguments[@]}" --test device_e2e --no-run

target_dir="$build_dir/target/$TARGET/debug"
test_binary="$(ls -t "$target_dir"/deps/device_e2e-* | grep -v '\.d$' | head -n 1)"

rm -rf -- "$root_dir"
mkdir -p -- "$root_dir"/{bin,dev,proc,sys,tmp}
cp -- /usr/bin/busybox "$root_dir/bin/busybox"
cp -- "$target_dir/keyway-router" "$root_dir/bin/keyway-router"
cp -- "$test_binary" "$root_dir/bin/device_e2e"

cat > "$root_dir/init" <<EOF
#!/bin/busybox sh
/bin/busybox --install -s /bin
mount -t proc proc /proc
mount -t sysfs sysfs /sys
mount -t devtmpfs devtmpfs /dev
mount -t tmpfs tmpfs /tmp
export KEYWAY_ISOLATED_VM=1 KEYWAY_ROUTER_BIN=/bin/keyway-router
if /bin/device_e2e --ignored --test-threads=1 --nocapture; then
    echo $PASS_MARKER
fi
poweroff -f
EOF
chmod +x -- "$root_dir/init"

(cd -- "$root_dir" && find . | cpio --quiet -o -H newc | gzip -1) > "$initrd_path"

timeout "$VM_TIMEOUT_SECONDS" qemu-system-x86_64 \
    -enable-kvm -cpu host -m "$VM_MEMORY_MB" \
    -kernel "$kernel_path" -initrd "$initrd_path" \
    -append 'console=ttyS0 quiet panic=-1' \
    -nographic -no-reboot -nic none \
    | tee "$log_path"

grep -q -- "$PASS_MARKER" "$log_path"
