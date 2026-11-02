#!/usr/bin/env bash
# A shared cache of the compiled third-party crates, so a fresh cloud
# container does not build all of them from scratch (several minutes on 4
# cores). It lives in the project's shared folder, one file per
# Cargo.lock + Cargo.toml + Rust toolchain, and holds no Grubcraft crates:
# those are always rebuilt from the checkout.
#
#   scripts/build-cache.sh restore   unpack the cache into target/ if target/ is empty
#   scripts/build-cache.sh save      pack target/'s third-party crates, if no cache
#                                    for this key exists yet
#
# scripts/pre-pr-check.sh runs both. Without the shared folder (on your own
# machine) it does nothing.
set -euo pipefail
cd "$(dirname "$0")/.."

dir=${GRUBCRAFT_CACHE_DIR:-/mnt/project-files/cache/cargo-target}
[ -d "$(dirname "$dir")" ] || exit 0

key=$( { cat Cargo.lock Cargo.toml rust-toolchain.toml; rustc -V; uname -m; } | sha256sum | cut -c1-16)
file="$dir/$key.tar.gz"

case "${1:-}" in
  restore)
    if [ -d target/debug/deps ] || [ -d target/release/deps ]; then
      exit 0
    fi
    if [ ! -f "$file" ]; then
      echo "build cache: none for this Cargo.lock yet ($key); building from scratch"
      exit 0
    fi
    echo "build cache: unpacking $key"
    mkdir -p target
    tar -xzf "$file" -C target
    ;;
  save)
    if [ -f "$file" ] || [ ! -d target/debug/deps ]; then
      exit 0
    fi
    # The workspace's own packages and targets: left out, they are rebuilt.
    names=$(cargo metadata --no-deps --format-version 1 | python3 -c '
import json, sys
d = json.load(sys.stdin)
names = {p["name"] for p in d["packages"]}
names |= {t["name"] for p in d["packages"] for t in p["targets"]}
print("\n".join(sorted({n.replace("-", "_") for n in names} | names)))')
    excludes=(--exclude='*/incremental' --exclude='*/examples' --exclude='bench' --exclude='stress'
      --exclude='shots' --exclude='balance' --exclude='tmp' --exclude='*.d')
    for n in $names; do
      for sub in deps .fingerprint build; do
        excludes+=(--exclude="*/$sub/$n-*" --exclude="*/$sub/lib$n-*")
      done
      excludes+=(--exclude="debug/$n" --exclude="release/$n" --exclude="debug/$n.*" --exclude="release/$n.*")
    done
    mkdir -p "$dir"
    tmp=$(mktemp -p "${TMPDIR:-/tmp}" build-cache.XXXXXX)
    echo "build cache: packing $key"
    tar -C target -czf "$tmp" "${excludes[@]}" $(cd target && ls -d debug release 2>/dev/null)
    size=$(stat -c %s "$tmp")
    if [ "$size" -gt $((4 * 1024 * 1024 * 1024)) ]; then
      echo "build cache: $((size / 1048576)) MB is too big, not saved"
      rm -f "$tmp"
      exit 0
    fi
    # Copy then rename, so a thread restoring meanwhile never sees half a file.
    cp "$tmp" "$file.part.$$" && mv "$file.part.$$" "$file"
    rm -f "$tmp"
    echo "build cache: saved $((size / 1048576)) MB"
    ;;
  *)
    sed -n '2,13p' "$0"
    exit 2
    ;;
esac
