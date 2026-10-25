#!/usr/bin/env python3
"""Compute the Nix cargoHash for flake.nix without building the package.

buildRustPackage's cargoHash is the recursive NAR hash of fetchCargoVendor's
vendor-staging directory: a copy of Cargo.lock plus
tarballs/<name>-<version>.tar.gz for every crates.io entry in the lock file.
Nix hashes that fixed-output derivation, not `cargo vendor` and not the
compiled package. Git dependencies are rejected: Nix fetches those with
nix-prefetch-git, and guessing their layout would publish a wrong hash.
"""

from __future__ import annotations

import base64
import hashlib
import os
import shutil
import stat
import sys
import tempfile
import time
import tomllib
import urllib.error
import urllib.request
from concurrent.futures import ThreadPoolExecutor, as_completed
from pathlib import Path

CRATES_IO = "registry+https://github.com/rust-lang/crates.io-index"
USER_AGENT = "nixpkgs-fetchCargoVendor/2 (https://github.com/NixOS/nixpkgs)"
WORKERS = 8


def write_bytes(digest: hashlib._Hash, data: bytes) -> None:
    digest.update(len(data).to_bytes(8, "little"))
    digest.update(data)
    pad = (8 - (len(data) % 8)) % 8
    if pad:
        digest.update(b"\0" * pad)


def write_file(digest: hashlib._Hash, path: Path, size: int) -> None:
    digest.update(size.to_bytes(8, "little"))
    with path.open("rb") as handle:
        while True:
            chunk = handle.read(1024 * 1024)
            if not chunk:
                break
            digest.update(chunk)
    pad = (8 - (size % 8)) % 8
    if pad:
        digest.update(b"\0" * pad)


def dump(digest: hashlib._Hash, path: Path) -> None:
    """Nix NAR node. The archive magic is written by nar_sri, not here."""
    write_bytes(digest, b"(")
    mode = path.lstat().st_mode
    if stat.S_ISLNK(mode):
        write_bytes(digest, b"type")
        write_bytes(digest, b"symlink")
        write_bytes(digest, b"target")
        write_bytes(digest, os.readlink(path).encode())
    elif stat.S_ISREG(mode):
        write_bytes(digest, b"type")
        write_bytes(digest, b"regular")
        if mode & stat.S_IXUSR:
            write_bytes(digest, b"executable")
            write_bytes(digest, b"")
        write_bytes(digest, b"contents")
        write_file(digest, path, path.lstat().st_size)
    elif stat.S_ISDIR(mode):
        write_bytes(digest, b"type")
        write_bytes(digest, b"directory")
        for child in sorted(path.iterdir(), key=lambda entry: entry.name.encode()):
            write_bytes(digest, b"entry")
            write_bytes(digest, b"(")
            write_bytes(digest, b"name")
            write_bytes(digest, child.name.encode())
            write_bytes(digest, b"node")
            dump(digest, child)
            write_bytes(digest, b")")
    else:
        raise RuntimeError(f"unsupported file type: {path}")
    write_bytes(digest, b")")


def nar_sri(path: Path) -> str:
    digest = hashlib.sha256()
    write_bytes(digest, b"nix-archive-1")
    dump(digest, path)
    return "sha256-" + base64.b64encode(digest.digest()).decode("ascii")


def download_tarball(name: str, version: str, checksum: str, dest: Path) -> None:
    url = f"https://static.crates.io/crates/{name}/{version}/download"
    last_error: Exception | None = None
    for attempt in range(5):
        try:
            request = urllib.request.Request(url, headers={"User-Agent": USER_AGENT})
            with urllib.request.urlopen(request, timeout=60) as response:
                hasher = hashlib.sha256()
                with dest.open("wb") as handle:
                    while True:
                        chunk = response.read(64 * 1024)
                        if not chunk:
                            break
                        handle.write(chunk)
                        hasher.update(chunk)
            os.chmod(dest, 0o644)
            actual = hasher.hexdigest()
            if actual != checksum:
                raise RuntimeError(
                    f"checksum mismatch for {name}-{version}: {actual} != {checksum}"
                )
            return
        except RuntimeError:
            dest.unlink(missing_ok=True)
            raise
        except (urllib.error.URLError, TimeoutError, OSError) as error:
            dest.unlink(missing_ok=True)
            last_error = error
            time.sleep(0.5 * (attempt + 1))
    raise RuntimeError(f"failed to fetch {url}: {last_error}")


def stage_vendor(lock_path: Path, out_dir: Path) -> None:
    """Same tree as nixpkgs fetch-cargo-vendor create-vendor-staging."""
    specs = _registry_crates(lock_path)
    out_dir.mkdir(parents=True, exist_ok=True)
    # Copy, matching shutil.copy: keep the lock file's executable bit.
    shutil.copy(lock_path, out_dir / "Cargo.lock")
    tarballs = out_dir / "tarballs"
    tarballs.mkdir()
    total = len(specs)
    print(f"Fetching {total} crates", file=sys.stderr)
    with ThreadPoolExecutor(max_workers=WORKERS) as pool:
        futures = [
            pool.submit(
                download_tarball,
                name,
                version,
                checksum,
                tarballs / f"{name}-{version}.tar.gz",
            )
            for name, version, checksum in specs
        ]
        done = 0
        for future in as_completed(futures):
            future.result()
            done += 1
            if done == total or done % 50 == 0:
                print(f"Fetched {done}/{total}", file=sys.stderr)


def _registry_crates(lock_path: Path) -> list[tuple[str, str, str]]:
    with lock_path.open("rb") as handle:
        lock = tomllib.load(handle)
    crates: dict[tuple[str, str], str] = {}
    for package in lock["package"]:
        source = package.get("source")
        if source is None:
            continue
        name = package["name"]
        version = package["version"]
        if source.startswith("git+"):
            raise SystemExit(
                f"git dependency {name} {version} is not supported; "
                "cargoHash would need nix-prefetch-git's tree"
            )
        if source != CRATES_IO:
            raise SystemExit(f"unsupported Cargo source for {name} {version}: {source}")
        checksum = package.get("checksum")
        if not checksum:
            raise SystemExit(f"missing checksum for {name} {version}")
        key = (name, version)
        previous = crates.get(key)
        if previous is not None and previous != checksum:
            raise SystemExit(f"checksum conflict for {name} {version}")
        crates[key] = checksum
    return [(name, version, checksum) for (name, version), checksum in crates.items()]


def cargo_hash(lock_path: Path) -> str:
    with tempfile.TemporaryDirectory(prefix="cargo-hash-") as tmp:
        staging = Path(tmp) / "vendor-staging"
        stage_vendor(lock_path, staging)
        return nar_sri(staging)


def main() -> None:
    lock_path = Path(sys.argv[1]) if len(sys.argv) > 1 else Path("Cargo.lock")
    if not lock_path.is_file():
        fallback = Path(__file__).resolve().parents[2] / "Cargo.lock"
        if fallback.is_file():
            lock_path = fallback
        else:
            raise SystemExit(f"Cargo.lock not found: {lock_path}")
    print(cargo_hash(lock_path))


if __name__ == "__main__":
    main()
