#!/usr/bin/env python3
"""Print the rust-version written in a Cargo.toml.

Nothing is printed when the manifest does not set one. Callers skip that
manifest. There is no default toolchain: the MSRV job must not invent one.

A direct ``rust-version = "..."`` on the package wins. ``rust-version.workspace
= true`` uses ``[workspace.package].rust-version``. If the package omits the
key entirely, ``[workspace.package].rust-version`` is the manifest's value.
"""

from __future__ import annotations

import sys
import tomllib
from pathlib import Path


def rust_version(path: Path) -> str | None:
    data = tomllib.loads(path.read_text(encoding="utf-8"))
    package = data.get("package") or {}
    workspace_package = (data.get("workspace") or {}).get("package") or {}
    raw = package.get("rust-version", None)
    package_omitted = "rust-version" not in package
    if isinstance(raw, dict):
        raw = workspace_package.get("rust-version") if raw.get("workspace") is True else None
    elif package_omitted:
        raw = workspace_package.get("rust-version")
    if isinstance(raw, str) and raw.strip():
        return raw.strip()
    return None


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit(f"usage: {sys.argv[0]} <Cargo.toml>")
    found = rust_version(Path(sys.argv[1]))
    if found:
        print(found)


if __name__ == "__main__":
    main()
