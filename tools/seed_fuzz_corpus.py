"""Seed each fuzz target from the immutable checked-in fixture corpus."""

from __future__ import annotations

import hashlib
from pathlib import Path

root = Path(__file__).resolve().parents[1]
fixtures = root / "crates/pytecode-engine/fixtures/classes"
for target in ("class_parse", "model_lift", "verify_bounded"):
    destination = root / "fuzz/corpus" / target
    destination.mkdir(parents=True, exist_ok=True)
    for source in sorted(fixtures.rglob("*.class")):
        data = source.read_bytes()
        if len(data) <= 262_144:
            (destination / hashlib.sha256(data).hexdigest()).write_bytes(data)
