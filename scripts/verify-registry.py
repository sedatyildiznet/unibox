#!/usr/bin/env python3
"""Fail CI when a native connector points at a missing or unsigned GitHub release asset."""
from __future__ import annotations

import json
import os
import sys
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
REGISTRY = ROOT / "registry" / "stable.json"


def github_json(url: str) -> dict:
    headers = {
        "Accept": "application/vnd.github+json",
        "User-Agent": "Unibox-CI",
    }
    token = os.getenv("GITHUB_TOKEN")
    if token:
        headers["Authorization"] = f"Bearer {token}"
    request = urllib.request.Request(url, headers=headers)
    with urllib.request.urlopen(request, timeout=20) as response:
        return json.load(response)


def main() -> int:
    connectors = json.loads(REGISTRY.read_text(encoding="utf-8"))
    ids: set[str] = set()
    ports: set[int] = set()
    errors: list[str] = []

    for connector in connectors:
        cid = connector["id"]
        port = int(connector["port"])
        if cid in ids:
            errors.append(f"duplicate connector id: {cid}")
        if port in ports:
            errors.append(f"duplicate connector port: {port}")
        ids.add(cid)
        ports.add(port)

        adapter = connector["adapter"]
        if adapter == "python-legacy":
            repo = github_json(f"https://api.github.com/repos/{connector['repository']}")
            if repo.get("archived"):
                errors.append(f"{cid}: upstream repository is archived")
            continue

        release = github_json(f"https://api.github.com/repos/{connector['repository']}/releases/latest")
        asset_name = connector["asset_linux_amd64"]
        asset = next((item for item in release.get("assets", []) if item.get("name") == asset_name), None)
        if not asset:
            errors.append(f"{cid}: {asset_name} missing from latest release {release.get('tag_name')}")
            continue
        digest = asset.get("digest") or ""
        if not digest.startswith("sha256:"):
            errors.append(f"{cid}: latest asset has no GitHub SHA-256 digest")

    if errors:
        print("Connector registry validation failed:", file=sys.stderr)
        for error in errors:
            print(f" - {error}", file=sys.stderr)
        return 1

    print(f"Validated {len(connectors)} connector definitions and their upstream release assets.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
