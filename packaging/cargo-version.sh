#!/bin/sh
# Print workspace.package.version from the repository Cargo.toml.
# release.yml and flatpak.yml both compare a tag against this value.
set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
sed -n 's/^version = "\(.*\)"/\1/p' "$root/Cargo.toml" | head -n 1
