#!/usr/bin/env bash
# Run cargo on the rust-version written in a manifest.
#
# rust-toolchain.toml pins 1.92.0. rustup would select that file for every
# cargo and rustc in this checkout, so an MSRV check would compile with
# 1.92.0 even when the manifest says something else. RUSTUP_TOOLCHAIN and
# `cargo +<ver>` both override the file; both are set so nested rustc
# processes stay on the manifest's toolchain.
#
# When the manifest has no rust-version, this prints a warning and exits 0.
# It does not substitute a version.
#
# Usage: msrv-cargo.sh <manifest> <cargo args...>
#   msrv-cargo.sh Cargo.toml check --manifest-path Cargo.toml --locked --all-targets

set -euo pipefail

manifest=${1:?usage: msrv-cargo.sh <manifest> <cargo args...>}
shift

script_dir=$(cd "$(dirname "$0")" && pwd)
version=$(python3 "$script_dir/rust-version.py" "$manifest")

if [ -z "$version" ]; then
  echo "::warning::${manifest} has no rust-version; skipping MSRV check"
  echo "${manifest} has no rust-version; skipping MSRV check" >&2
  exit 0
fi

echo "MSRV ${version} for ${manifest}"
echo "rust-toolchain.toml pins 1.92.0; overriding with RUSTUP_TOOLCHAIN=${version} and cargo +${version}"

rustup toolchain install "$version" --profile minimal --no-self-update

export RUSTUP_TOOLCHAIN="$version"
actual=$(cargo "+${version}" rustc --version)
echo "rustc: ${actual}"
reported=${actual#rustc }
reported=${reported%% *}
case "$reported" in
  "$version"|"$version."*) ;;
  *)
    echo "expected rustc ${version}, got: ${actual}" >&2
    exit 1
    ;;
esac

exec cargo "+${version}" "$@"
