#!/usr/bin/env python3
"""Generates a believable home folder for screenshots and manual testing.

    python3 scripts/demo-tree.py <empty-directory>

The tree is written to <empty-directory>/Users/alex. Every file is sparse, so the tree
reports a few hundred gigabytes while using almost no disk space. The output is the same
on every run.
"""

import os
import random
import sys

GB = 1024**3
MB = 1024**2

# (name, size in GB, children). A folder without children is filled with generated content.
HOME = [
    ("Movies", 182, [
        ("Final Cut Projects", 118, [
            ("Iceland Documentary.fcpbundle", 61, None),
            ("Wedding - Hana & Tom.fcpbundle", 34, None),
            ("Product Launch.fcpbundle", 23, None),
        ]),
        ("Exports", 41, None),
        ("Screen Recordings", 23, None),
    ]),
    ("Pictures", 118, [
        ("Photos Library.photoslibrary", 96, [
            ("originals", 71, None),
            ("resources", 19, None),
            ("database", 6, None),
        ]),
        ("Lightroom", 22, None),
    ]),
    ("Library", 96, [
        ("Developer", 44, [
            ("Xcode", 27, [
                ("DerivedData", 16, None),
                ("iOS DeviceSupport", 8, None),
                ("Archives", 3, None),
            ]),
            ("CoreSimulator", 17, None),
        ]),
        ("Application Support", 24, None),
        ("Caches", 15, None),
        ("Containers", 9, None),
        ("Mail", 4, None),
    ]),
    ("Developer", 64, [
        ("storefront", 21, [
            ("node_modules", 12, None),
            (".next", 5, None),
            (".git", 3, None),
            ("src", 1, None),
        ]),
        ("render-engine", 19, [
            ("target", 15, None),
            (".git", 3, None),
            ("crates", 1, None),
        ]),
        ("ml-experiments", 16, None),
        ("dotfiles", 8, None),
    ]),
    ("Music", 41, [
        ("Logic", 24, None),
        ("Music Library", 17, None),
    ]),
    ("Documents", 27, None),
    ("Downloads", 23, None),
    (".Trash", 9, None),
    ("Desktop", 6, None),
]

FOLDER_NAMES = [
    "Archive", "Assets", "Audio", "Backups", "Cache", "Clips", "Drafts", "Footage", "Index",
    "Masters", "Media", "Models", "Old", "Originals", "Previews", "Proxies", "Raw", "Renders",
    "Samples", "Sessions", "Shared", "Snapshots", "Sources", "Stems", "Thumbnails", "Versions",
]
EXTENSIONS = [".mov", ".mp4", ".dng", ".heic", ".zip", ".dmg", ".wav", ".psd", ".bin", ".db"]


def shares(rng, count):
    """Random shares summing to one, skewed so a few items dominate, as on real disks."""
    weights = sorted((rng.paretovariate(1.1) for _ in range(count)), reverse=True)
    total = sum(weights)
    return [weight / total for weight in weights]


def write_file(path, size):
    with open(path, "wb") as handle:
        handle.truncate(max(int(size), 1))


def fill(rng, path, size, depth):
    """Fills `path` with generated folders and files adding up to roughly `size` bytes."""
    os.makedirs(path, exist_ok=True)

    folder_count = rng.randint(2, 7) if depth < 4 and size > 400 * MB else 0
    file_count = rng.randint(3, 14)
    folder_share = rng.uniform(0.55, 0.9) if folder_count else 0.0

    names = rng.sample(FOLDER_NAMES, folder_count)
    for name, share in zip(names, shares(rng, folder_count)):
        fill(rng, os.path.join(path, name), size * folder_share * share, depth + 1)

    for index, share in enumerate(shares(rng, file_count)):
        name = f"{rng.choice(['IMG', 'clip', 'export', 'data', 'take'])}_{rng.randint(1000, 9999)}_{index}"
        write_file(os.path.join(path, name + rng.choice(EXTENSIONS)), size * (1 - folder_share) * share)


def build(rng, path, entries):
    os.makedirs(path, exist_ok=True)
    for name, gigabytes, children in entries:
        child = os.path.join(path, name)
        if children is None:
            fill(rng, child, gigabytes * GB, 1)
        else:
            build(rng, child, children)
            # Loose files next to the named folders keep the totals from looking too tidy.
            write_file(os.path.join(child, ".DS_Store"), 8 * 1024)


def main():
    if len(sys.argv) != 2:
        sys.exit(__doc__)

    destination = sys.argv[1]
    if os.path.exists(destination) and os.listdir(destination):
        sys.exit(f"{destination} is not empty; refusing to write into it.")

    home = os.path.join(destination, "Users", "alex")
    build(random.Random(1987), home, HOME)
    print(f"Wrote demo tree to {home}")


if __name__ == "__main__":
    main()
