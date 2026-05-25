#!/usr/bin/env python3
from __future__ import annotations

import argparse
import re
import tomllib
from pathlib import Path


PATH_DEP_VERSION_RE = re.compile(r'(path = "\.\./[^"]+", version = ")([^"]+)(")')


def next_minor_version(current: str) -> str:
    major, minor, _patch = map(int, current.split("."))
    return f"{major}.{minor + 1}.0"


def update_workspace_manifest(path: Path, current: str, updated: str) -> None:
    manifest_text = path.read_text(encoding="utf-8")
    updated_text, replacements = re.subn(
        rf'(^version = "){re.escape(current)}(")$',
        rf"\g<1>{updated}\2",
        manifest_text,
        count=1,
        flags=re.MULTILINE,
    )
    if replacements != 1:
        raise RuntimeError(f"failed to update workspace version in {path}")
    path.write_text(updated_text, encoding="utf-8")


def update_crate_manifest(path: Path, updated: str) -> None:
    manifest_text = path.read_text(encoding="utf-8")
    updated_text = PATH_DEP_VERSION_RE.sub(rf"\g<1>{updated}\3", manifest_text)
    if updated_text != manifest_text:
        path.write_text(updated_text, encoding="utf-8")


def update_lockfile(path: Path, crate_names: list[str], updated: str) -> None:
    lock_text = path.read_text(encoding="utf-8")

    for crate_name in crate_names:
        lock_text, replacements = re.subn(
            rf'(\[\[package\]\]\nname = "{re.escape(crate_name)}"\nversion = ")([^"]+)(")',
            rf"\g<1>{updated}\3",
            lock_text,
            count=1,
        )
        if replacements != 1:
            raise RuntimeError(f"failed to update {crate_name} version in {path}")

    path.write_text(lock_text, encoding="utf-8")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--write", action="store_true")
    parser.add_argument("--github-output")
    args = parser.parse_args()

    repo_root = Path(__file__).resolve().parents[2]
    workspace_manifest = repo_root / "Cargo.toml"
    lockfile = repo_root / "Cargo.lock"
    cargo = tomllib.loads(workspace_manifest.read_text(encoding="utf-8"))
    current = cargo["workspace"]["package"]["version"]
    updated = next_minor_version(current)
    crate_manifests = sorted((repo_root / "crates").glob("*/Cargo.toml"))
    crate_names = [
        tomllib.loads(manifest.read_text(encoding="utf-8"))["package"]["name"]
        for manifest in crate_manifests
    ]

    if args.write:
        update_workspace_manifest(workspace_manifest, current, updated)
        for manifest in crate_manifests:
            update_crate_manifest(manifest, updated)
        update_lockfile(lockfile, crate_names, updated)

    if args.github_output:
        with Path(args.github_output).open("a", encoding="utf-8") as github_output:
            github_output.write(f"version={updated}\n")
            github_output.write(f"tag=v{updated}\n")

    print(updated)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
