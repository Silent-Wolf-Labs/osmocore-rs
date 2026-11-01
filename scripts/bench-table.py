#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Append a saved Criterion baseline to the table in docs/benchmarks.md as a new column.

Usage, from the repository root: scripts/bench-table.py BASELINE "COLUMN TITLE"
"""

import json
import pathlib
import sys

CRITERION = pathlib.Path("osmocore-rs/target/criterion")
DOCUMENT = pathlib.Path("docs/benchmarks.md")
BEGIN, END = "<!-- bench-table:begin -->", "<!-- bench-table:end -->"


def medians(baseline):
    """Map each benchmark's full ID to its median time in nanoseconds."""
    result = {}
    for path in CRITERION.glob(f"**/{baseline}/benchmark.json"):
        full_id = json.loads(path.read_text())["full_id"]
        estimates = json.loads(path.with_name("estimates.json").read_text())
        result[full_id] = estimates["median"]["point_estimate"]
    if not result:
        sys.exit(f"no saved baseline named {baseline!r} under {CRITERION}")
    return result


def cells(line):
    return [cell.strip() for cell in line.strip().strip("|").split("|")]


def row(values):
    return "| " + " | ".join(values) + " |"


def main():
    if len(sys.argv) != 3:
        sys.exit(__doc__)
    baseline, title = sys.argv[1:]
    new = medians(baseline)
    text = DOCUMENT.read_text()
    before, rest = text.split(BEGIN + "\n")
    table, after = rest.split(END)
    lines = table.strip().splitlines()
    header = cells(lines[0])
    rows = {cells(line)[0].strip("`"): cells(line)[1:] for line in lines[2:]}
    width = len(header) - 1
    output = [row(header + [title]), row(["---"] + ["---:"] * (width + 1))]
    for name in sorted(rows.keys() | new.keys()):
        value = new.get(name)
        cell = "-" if value is None else f"{value:,.1f}" if value < 100 else f"{value:,.0f}"
        output.append(row([f"`{name}`"] + rows.get(name, ["-"] * width) + [cell]))
    DOCUMENT.write_text(before + BEGIN + "\n" + "\n".join(output) + "\n" + END + after)


if __name__ == "__main__":
    main()
