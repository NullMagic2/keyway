#!/usr/bin/env bash
# Module: keyway.clean_source
# Purpose: After confirmation, delete everything except bin/ and packages/.
# Created: 2026-11-08
# Architecture: Removes this script too; .git stays unless --include-git is given.
set -euo pipefail

project_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
source "$project_dir/tools/script_common.sh"

readonly CONFIRMATION_WORD='delete'
readonly SOURCE_PATHS=(Cargo.toml Cargo.lock crates extension packaging data)

bin_dir="$project_dir/bin"
assume_yes=false
include_git=false

for argument in "$@"; do
    case "$argument" in
        --yes) assume_yes=true ;;
        --include-git) include_git=true ;;
        *) fail "unknown option: $argument (expected --yes or --include-git)" ;;
    esac
done

require_project_root
acquire_project_lock

if [[ -L "$bin_dir" ]]; then
    fail "bin is a symlink: $bin_dir"
fi

if ((${#BINARY_NAMES[@]} == 0)); then
    fail 'no binary crates exist yet, so nothing would be left'
fi

# Deleting source is only safe once the binaries it produces exist and are current.
oldest_binary=''

for binary_name in "${BINARY_NAMES[@]}"; do
    binary_path="$bin_dir/$binary_name"

    if [[ ! -f "$binary_path" || ! -x "$binary_path" ]]; then
        fail "missing binary: $binary_path (run build.sh first)"
    fi

    if [[ -z "$oldest_binary" || "$binary_path" -ot "$oldest_binary" ]]; then
        oldest_binary="$binary_path"
    fi
done

for source_name in "${SOURCE_PATHS[@]}"; do
    source_path="$project_dir/$source_name"

    if [[ -e "$source_path" ]] && [[ -n "$(find "$source_path" -type f -newer "$oldest_binary" -print -quit)" ]]; then
        fail "$source_name changed after the last build (run build.sh first)"
    fi
done

# Keeping .git only protects work that has actually been committed.
if [[ -e "$project_dir/.git" ]]; then
    if [[ -n "$(git -C "$project_dir" status --porcelain)" ]]; then
        fail 'uncommitted changes would be lost (commit them first)'
    fi
else
    printf 'Warning: no .git here, so this cannot be undone.\n'
fi

find_filter=(-mindepth 1 -maxdepth 1 ! -name bin ! -name packages)

if [[ "$include_git" == false ]]; then
    find_filter+=(! -name .git)
elif [[ -e "$project_dir/.git" ]]; then
    printf 'Warning: --include-git also deletes the commit history.\n'
fi

printf 'Project: %s\nKeeping: bin/ and packages/\nDeleting:\n' "$project_dir"
find "$project_dir" "${find_filter[@]}" -printf '  %P\n' | sort

if [[ "$assume_yes" == false ]]; then
    if [[ ! -t 0 ]]; then
        fail 'no terminal to confirm on (use --yes to skip the prompt)'
    fi

    read -r -p "Type '$CONFIRMATION_WORD' to continue: " answer

    if [[ "$answer" != "$CONFIRMATION_WORD" ]]; then
        printf 'Cancelled; nothing was deleted.\n'
        exit 0
    fi
fi

find "$project_dir" "${find_filter[@]}" -exec rm -rf -- {} +
printf 'Done. Binaries are in %s\n' "$bin_dir"
