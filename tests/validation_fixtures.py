"""Validation fixture registry and multi-release test matrix.

Single source of truth for all validation tier tests. Maps each Java fixture
to its minimum required ``--release`` level and generates the cross-product
of (fixture, release) pairs for parametrized testing.
"""

from __future__ import annotations

from tests.helpers import FIXTURE_MIN_RELEASES, VALIDATION_RELEASES, list_java_resources

# All fixtures with their minimum release (from FIXTURE_MIN_RELEASES; default 8)
VALIDATION_FIXTURES: list[tuple[str, int]] = sorted(
    (name, FIXTURE_MIN_RELEASES.get(name.rsplit("/", 1)[-1], 8)) for name in list_java_resources(max_release=25)
)


def validation_matrix() -> list[tuple[str, int]]:
    """Generate all valid ``(fixture_name, release)`` pairs.

    Each fixture is compiled at every release level from
    :data:`VALIDATION_RELEASES` that is >= the fixture's minimum release.
    """
    pairs: list[tuple[str, int]] = []
    for fixture_name, min_release in VALIDATION_FIXTURES:
        for release in VALIDATION_RELEASES:
            if release >= min_release:
                pairs.append((fixture_name, release))
    return pairs
