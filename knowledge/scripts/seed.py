#!/usr/bin/env python3
"""Seed the k-stack knowledge graph by driving the binary over stdio JSON-RPC.

Reads knowledge/graph/seed.jsonl, stores each node via `ket_store`, resolves
parent ids to CIDs, and writes knowledge/graph/cids.lock (committed alongside
the manifest so divergence is detectable by diff).

Usage:  KET_HOME=.ket python3 knowledge/scripts/seed.py
"""
from __future__ import annotations

import json
import os
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
MANIFEST = ROOT / "knowledge" / "graph" / "seed.jsonl"
LOCKFILE = ROOT / "knowledge" / "graph" / "cids.lock"
NODESFILE = ROOT / "knowledge" / "graph" / "nodes.run"
BINARY = ROOT / "k-stack" / "target" / "release" / "k-stack"

# Domain kind -> k-stack NodeKind
KIND_MAP = {
    "schema": "cdom",
    "decision": "reasoning",
    "invariant": "reasoning",
    "component": "code",
    "event": "memory",
    "context": "context",
    "constraint": "context",
    "question": "task",
    "finding": "reasoning",
    "pattern": "reasoning",
}


def main() -> int:
    if not BINARY.exists():
        print(f"error: k-stack binary missing at {BINARY}", file=sys.stderr)
        print("hint: (cd k-stack && cargo build --release)", file=sys.stderr)
        return 1

    env = os.environ.copy()
    env.setdefault("KET_HOME", str(ROOT / ".ket"))

    proc = subprocess.Popen(
        [str(BINARY)],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        env=env,
        text=True,
        bufsize=1,
    )

    req_id = 0

    def call(method: str, params: dict) -> dict:
        nonlocal req_id
        req_id += 1
        msg = {"jsonrpc": "2.0", "id": req_id, "method": method, "params": params}
        assert proc.stdin and proc.stdout
        proc.stdin.write(json.dumps(msg) + "\n")
        proc.stdin.flush()
        line = proc.stdout.readline()
        if not line:
            err = proc.stderr.read() if proc.stderr else ""
            raise RuntimeError(f"no response from k-stack: {err}")
        resp = json.loads(line)
        if "error" in resp and resp["error"]:
            raise RuntimeError(f"{method} failed: {resp['error']}")
        return resp.get("result", {})

    # MCP handshake
    call("initialize", {"protocolVersion": "2024-11-05", "capabilities": {}})

    cid_by_id: dict[str, str] = {}
    entries: list[dict] = []

    with MANIFEST.open() as f:
        for raw in f:
            raw = raw.strip()
            if not raw:
                continue
            rec = json.loads(raw)
            domain_kind = rec["kind"]
            node_kind = KIND_MAP[domain_kind]
            ident = rec.get("id") or rec.get("ref")

            if domain_kind in ("schema", "decision", "context", "question"):
                content = (ROOT / "knowledge" / rec["ref"]).read_text()
            elif domain_kind == "invariant":
                content = json.dumps(
                    {
                        "id": rec["id"],
                        "statement": rec["statement"],
                        "penalty": rec.get("penalty"),
                    },
                    sort_keys=True,
                )
            elif domain_kind == "constraint":
                content = json.dumps(
                    {
                        "id": rec["id"],
                        "statement": rec["statement"],
                        "kind": rec.get("ckind"),
                        "domain": rec.get("domain"),
                    },
                    sort_keys=True,
                )
            elif domain_kind == "finding":
                content = json.dumps(
                    {
                        "id": rec["id"],
                        "claim": rec["claim"],
                        "evidence": rec.get("evidence", ""),
                        "sources": rec.get("sources", []),
                        "confidence": rec["confidence"],
                        "accessed_at": rec["accessed_at"],
                    },
                    sort_keys=True,
                )
            elif domain_kind == "pattern":
                content = json.dumps(
                    {
                        "id": rec["id"],
                        "pattern": rec["pattern"],
                    },
                    sort_keys=True,
                )
            else:
                content = json.dumps(rec, sort_keys=True)

            parents_cids = [cid_by_id[p] for p in rec.get("parents", [])]

            result = call(
                "tools/call",
                {
                    "name": "ket_store",
                    "arguments": {
                        "content": content,
                        "kind": node_kind,
                        "parents": parents_cids,
                        "agent": "seed.py",
                    },
                },
            )

            # MCP wraps tool results in content[] blocks
            if "content" in result and isinstance(result["content"], list):
                inner = json.loads(result["content"][0]["text"])
            else:
                inner = result

            node_cid = inner["node_cid"]
            content_cid = inner["content_cid"]
            cid_by_id[ident] = node_cid  # node_cid used for DAG parent linkage
            entries.append(
                {
                    "id": ident,
                    "domain_kind": domain_kind,
                    "node_kind": node_kind,
                    "content_cid": content_cid,
                    "node_cid": node_cid,
                    "parents": rec.get("parents", []),
                }
            )
            print(f"{domain_kind:10}  {ident:40}  {content_cid[:16]}...")

    proc.stdin.close()
    proc.wait(timeout=5)

    # cids.lock: deterministic snapshot — content_cid + symbolic parent ids.
    # Sorted by id for diff stability. This is the divergence oracle.
    lock_entries = sorted(
        (
            {
                "id": e["id"],
                "domain_kind": e["domain_kind"],
                "node_kind": e["node_kind"],
                "content_cid": e["content_cid"],
                "parents": e["parents"],
            }
            for e in entries
        ),
        key=lambda e: e["id"],
    )
    LOCKFILE.write_text(
        "# AUTOGENERATED by knowledge/scripts/seed.py — do not edit by hand.\n"
        "# content_cid is deterministic: diff between peers reveals divergent content.\n"
        "# parents are symbolic ids (resolved via this file). Sorted by id.\n"
        + "\n".join(json.dumps(e, sort_keys=True) for e in lock_entries)
        + "\n"
    )

    # nodes.run: ephemeral per-run node_cid handles (for live MCP calls).
    # NOT committed; gitignored. Regenerated on every seed.
    NODESFILE.write_text(
        "# EPHEMERAL — regenerated each seed run. node_cids include a timestamp\n"
        "# and change across runs. Use cids.lock for divergence detection.\n"
        + "\n".join(
            json.dumps({"id": e["id"], "node_cid": e["node_cid"]}, sort_keys=True)
            for e in entries
        )
        + "\n"
    )
    print(f"\nwrote {LOCKFILE.relative_to(ROOT)} ({len(entries)} nodes, deterministic)")
    print(f"wrote {NODESFILE.relative_to(ROOT)} (ephemeral node handles)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
