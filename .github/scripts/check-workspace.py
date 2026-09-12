#!/usr/bin/env python3
"""Fail if crates sit outside the workspace or omit lint inheritance."""

import json
import subprocess
import sys
import tomllib
from pathlib import Path

SKIP_DIRS = {".git", "target", "fuzz"}


def main() -> int:
    meta = json.loads(
        subprocess.check_output(
            [
                "cargo",
                "metadata",
                "--no-deps",
                "--format-version",
                "1",
                "--offline",
            ],
            text=True,
        )
    )
    workspace_root = Path(meta["workspace_root"]).resolve()
    member_manifests = {Path(pkg["manifest_path"]).resolve() for pkg in meta["packages"]}
    root_manifest = workspace_root / "Cargo.toml"
    errors: list[str] = []

    for manifest in iter_files(workspace_root, "Cargo.toml"):
        if manifest == root_manifest:
            continue
        if manifest not in member_manifests:
            errors.append(
                f"{rel(manifest, workspace_root)}: not a workspace member. "
                "Add it to workspace.members in the root Cargo.toml."
            )

    if not meta["packages"]:
        rust_files = list(iter_suffix(workspace_root, ".rs"))
        if rust_files:
            listed = ", ".join(rel(path, workspace_root) for path in rust_files)
            errors.append(
                "Rust sources exist but the workspace has no packages: "
                f"{listed}. List the crate in workspace.members."
            )

    for pkg in meta["packages"]:
        manifest_path = Path(pkg["manifest_path"])
        data = tomllib.loads(manifest_path.read_text(encoding="utf-8"))
        lints = data.get("lints")
        if not isinstance(lints, dict) or lints.get("workspace") is not True:
            errors.append(
                f"{rel(manifest_path, workspace_root)}: missing "
                "[lints] workspace = true. The crate will not inherit the "
                "workspace maximum lint preset."
            )

    if errors:
        print("Workspace check failed:", file=sys.stderr)
        for error in errors:
            print(f"  {error}", file=sys.stderr)
        return 1
    return 0


def iter_files(root: Path, name: str):
    for path in root.rglob(name):
        if not skip(path):
            yield path.resolve()


def iter_suffix(root: Path, suffix: str):
    for path in root.rglob(f"*{suffix}"):
        if not skip(path):
            yield path.resolve()


def skip(path: Path) -> bool:
    return any(part in SKIP_DIRS for part in path.parts)


def rel(path: Path, root: Path) -> str:
    try:
        return path.resolve().relative_to(root.resolve()).as_posix()
    except ValueError:
        return path.as_posix()


if __name__ == "__main__":
    sys.exit(main())
