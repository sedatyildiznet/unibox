#!/usr/bin/env python3
"""Validate connector registry entries against upstream GitHub repositories/releases."""
from __future__ import annotations

import json
import os
import sys
import urllib.error
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
    if not isinstance(connectors, list):
        print("Connector registry must be a top-level JSON array.", file=sys.stderr)
        return 1

    ids: set[str] = set()
    ports: set[int] = set()
    errors: list[str] = []

    for connector in connectors:
        cid = connector.get("id", "<missing-id>")
        repo_name = connector.get("repository", "")
        try:
            port = int(connector["port"])
        except (KeyError, TypeError, ValueError):
            errors.append(f"{cid}: missing or invalid port")
            continue

        if cid in ids:
            errors.append(f"duplicate connector id: {cid}")
        if port in ports:
            errors.append(f"duplicate connector port: {port}")
        ids.add(cid)
        ports.add(port)

        if not repo_name or "/" not in repo_name:
            errors.append(f"{cid}: invalid repository '{repo_name}'")
            continue

        adapter = connector.get("adapter", "")
        try:
            if adapter in {"python-legacy", "source-go"}:
                repo = github_json(f"https://api.github.com/repos/{repo_name}")
                if repo.get("archived"):
                    errors.append(f"{cid}: upstream repository {repo_name} is archived")
                if repo.get("disabled"):
                    errors.append(f"{cid}: upstream repository {repo_name} is disabled")
                continue

            release = github_json(f"https://api.github.com/repos/{repo_name}/releases/latest")
            asset_name = connector.get("asset_linux_amd64", "")
            asset = next((item for item in release.get("assets", []) if item.get("name") == asset_name), None)
            if not asset:
                available = ", ".join(item.get("name", "") for item in release.get("assets", [])[:12]) or "<none>"
                errors.append(
                    f"{cid}: asset '{asset_name}' missing from {repo_name} release "
                    f"{release.get('tag_name')}; available: {available}"
                )
                continue
            digest = asset.get("digest") or ""
            if not digest.startswith("sha256:"):
                errors.append(f"{cid}: {repo_name}/{asset_name} has no GitHub SHA-256 digest")
        except urllib.error.HTTPError as exc:
            errors.append(f"{cid}: GitHub API returned HTTP {exc.code} for {repo_name}")
        except urllib.error.URLError as exc:
            errors.append(f"{cid}: GitHub API network error for {repo_name}: {exc.reason}")
        except Exception as exc:
            errors.append(f"{cid}: unexpected validation error for {repo_name}: {exc}")

    if errors:
        print("Connector registry validation failed:", file=sys.stderr)
        for error in errors:
            print(f" - {error}", file=sys.stderr)
        return 1

    print(f"Validated {len(connectors)} connector definitions and their upstream release assets/repositories.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
