#!/usr/bin/env python3
"""Fail when resolved UI crate features exceed the native-package baseline.

Ceilings live only in ``.github/ui-baseline.env``. ``cargo metadata --locked``
reports the same feature set as ``cargo tree -e features``. The platform
filter is the Linux triple the native packages are built for, so a macOS-only
feature does not show up here.
"""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
BASELINE_PATH = ROOT / ".github" / "ui-baseline.env"
REQUIRED = ("GTK4_MAX", "LIBADWAITA_MAX", "RELM4_MAX_GNOME")

V4 = re.compile(r"^v4_(\d+)$")
V1 = re.compile(r"^v1_(\d+)$")
GNOME = re.compile(r"^gnome_(\d+)$")


def load_baseline(path: Path) -> dict[str, int]:
    values: dict[str, int] = {}
    for line_no, raw in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        line = raw.split("#", 1)[0].strip()
        if not line:
            continue
        if "=" not in line:
            raise SystemExit(f"{path}:{line_no}: expected KEY=VALUE")
        key, value = (part.strip() for part in line.split("=", 1))
        if not value.isdigit():
            raise SystemExit(f"{path}:{line_no}: {key} must be a non-negative integer")
        values[key] = int(value)
    missing = [key for key in REQUIRED if key not in values]
    if missing:
        raise SystemExit(f"{path}: missing {', '.join(missing)}")
    return values


def cargo_metadata(manifest: Path) -> dict:
    command = [
        "cargo",
        "metadata",
        "--format-version",
        "1",
        "--locked",
        "--filter-platform",
        "x86_64-unknown-linux-gnu",
        "--manifest-path",
        str(manifest),
    ]
    completed = subprocess.run(
        command,
        cwd=ROOT,
        check=False,
        text=True,
        capture_output=True,
    )
    if completed.returncode != 0:
        sys.stderr.write(completed.stderr)
        sys.stderr.write(completed.stdout)
        raise SystemExit(completed.returncode)
    return json.loads(completed.stdout)


def nodes_named(metadata: dict, name: str) -> list[tuple[str, set[str]]]:
    # Cargo 1.92 package ids look like
    # registry+https://github.com/rust-lang/crates.io-index#gtk4@0.10.3
    # The packages table is the reliable name source.
    names = {package["id"]: package["name"] for package in metadata["packages"]}
    found = []
    for node in metadata["resolve"]["nodes"]:
        if names.get(node["id"]) == name:
            found.append((node["id"], set(node.get("features") or [])))
    return found


def node_label(node_id: str, fallback: str) -> str:
    if "#" in node_id and "@" in node_id.split("#", 1)[1]:
        name, version = node_id.split("#", 1)[1].split("@", 1)
        return f"{name} {version}"
    return fallback


def numbered(features: set[str], pattern: re.Pattern[str]) -> list[tuple[int, str]]:
    matched = []
    for feature in features:
        hit = pattern.fullmatch(feature)
        if hit:
            matched.append((int(hit.group(1)), feature))
    return sorted(matched)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--manifest-path",
        default="Cargo.toml",
        help="Workspace manifest. login-helper has its own lockfile.",
    )
    parser.add_argument(
        "--require",
        default="",
        help="Comma-separated package names that must appear in the resolve.",
    )
    args = parser.parse_args()
    required = [name for name in args.require.split(",") if name]
    manifest = Path(args.manifest_path)
    if not manifest.is_absolute():
        manifest = ROOT / manifest

    baseline = load_baseline(BASELINE_PATH)
    gtk_max = baseline["GTK4_MAX"]
    adw_max = baseline["LIBADWAITA_MAX"]
    gnome_max = baseline["RELM4_MAX_GNOME"]
    print(
        "baseline: "
        f"gtk4 <= v4_{gtk_max}, "
        f"libadwaita <= v1_{adw_max}, "
        f"relm4 <= gnome_{gnome_max} "
        f"({BASELINE_PATH.relative_to(ROOT)})"
    )
    print(f"manifest: {manifest.relative_to(ROOT)}")

    metadata = cargo_metadata(manifest)
    checks = (
        ("gtk4", V4, gtk_max),
        ("libadwaita", V1, adw_max),
        ("relm4", GNOME, gnome_max),
    )
    violations: list[str] = []
    for name, pattern, ceiling in checks:
        packages = nodes_named(metadata, name)
        if not packages:
            message = f"{name}: not in this resolve"
            print(message)
            if name in required:
                violations.append(f"{message} (required)")
            continue
        for node_id, features in packages:
            present = [feature for _, feature in numbered(features, pattern)]
            print(
                f"{node_label(node_id, name)}: "
                f"{', '.join(present) if present else '(no version feature)'}"
            )
            for number, feature in numbered(features, pattern):
                if number > ceiling:
                    violations.append(
                        f"{node_id} enables {feature}, above the baseline"
                    )
    if violations:
        print("feature ceiling exceeded:", file=sys.stderr)
        for item in violations:
            print(f"  {item}", file=sys.stderr)
        raise SystemExit(1)
    print("feature ceiling ok")


if __name__ == "__main__":
    main()
