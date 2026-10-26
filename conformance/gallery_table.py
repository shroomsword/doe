#!/usr/bin/env python3
"""Keeps conformance/gallery_samples.tsv, the table of the Kaitai Struct gallery:
one row for each definition of the submodule, and what file it is tested with.

    python3 conformance/gallery_table.py           # add rows for new definitions, drop rows of removed ones
    python3 conformance/gallery_table.py --check   # change nothing; fail if the table is out of date
    python3 conformance/gallery_table.py --mark STATUS SOURCE NOTE FORMAT...
                                                   # fill the rows of formats that now have samples
    python3 conformance/gallery_table.py --readme  # write the table of formats into the README
    python3 conformance/gallery_table.py --tests   # write the list of tests of conformance.rs

The columns are tab-separated:

    format    the definition without .ksy, as the gallery has it (image/png)
    status    real      a file written by the tool that makes the format normally
              crafted   a file built by hand by make_gallery_samples.py, checked
                        against the reference compiler; valid, but not from the world
              failing   there is a file, and doe does not read it as it should; the
                        note has the issue (#N). Its files are not in the test
                        until it is fixed
              part-of   only imported by other definitions (the gallery's common/)
              none      no sample yet
    sample    the directory of the samples under samples/gallery/, or -
    source    where the files come from
    licence   what allows them to be in this repository
    note      what is worth knowing: which formats use a part-of definition,
              why there is no sample, or the issue of a format that fails

A new definition gets a row with status `none`; the rest of a row is written by
hand, and is never changed by this script.
"""

import re
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
GALLERY = HERE / "kaitai_struct_formats"
TABLE = HERE / "gallery_samples.tsv"
HEADER = ["format", "status", "sample", "source", "licence", "note"]
STATUSES = {"real", "crafted", "failing", "part-of", "none"}

# What the rows that were there before the table are, for the first run.
SCRIPT = "make_gallery_samples.py"
OURS = "MIT (this repository)"
SEED = {
    "archive/gzip": ("real", "gzip module of Python, run by " + SCRIPT),
    "archive/zip": ("real", "zipfile module of Python, run by " + SCRIPT),
    "executable/elf": ("real", "the C compiler (cc), run by " + SCRIPT),
    "executable/mach_o": ("real", "clang for the macOS targets, run by " + SCRIPT),
    "database/dbf": ("crafted", "built by " + SCRIPT),
    "executable/java_class": ("crafted", "built by " + SCRIPT),
    "executable/microsoft_pe": ("crafted", "built by " + SCRIPT),
    "image/bmp": ("crafted", "built by " + SCRIPT),
    "image/gif": ("crafted", "built by " + SCRIPT),
    "image/jpeg": ("crafted", "built by " + SCRIPT),
    "image/png": ("crafted", "built by " + SCRIPT),
    "media/wav": ("crafted", "built by " + SCRIPT),
    "network/ethernet_frame": ("crafted", "built by " + SCRIPT),
    "network/pcap": ("crafted", "built by " + SCRIPT),
}
NOTES = {
    "database/sqlite3": "left out of the conformance suite on purpose: the definition reads every column as a string, which only works in the reference's JavaScript runtime (see the README)",
    "network/ethernet_frame": "with the formats it imports (ipv4_packet, tcp_segment, udp_datagram, ...), which have no sample of their own",
}


def definitions():
    found = []
    for path in sorted(GALLERY.rglob("*.ksy")):
        relative = path.relative_to(GALLERY)
        if any(part.startswith((".", "_")) for part in relative.parts):
            continue
        found.append(relative.with_suffix("").as_posix())
    return found


def imports_of(name):
    """The definitions that `name` imports, as paths without .ksy."""
    text = (GALLERY / f"{name}.ksy").read_text(encoding="utf-8")
    lines = text.splitlines()
    out = []
    for i, line in enumerate(lines):
        if re.match(r"^\s*imports:\s*$", line):
            indent = len(line) - len(line.lstrip())
            for item in lines[i + 1 :]:
                match = re.match(r"^(\s*)-\s+['\"]?([^'\"#\s]+)", item)
                if match:
                    target = match.group(2)
                    if target.startswith("/"):
                        out.append(target[1:])
                    else:
                        parent = Path(name).parent
                        out.append((parent / target).as_posix())
                elif item.strip() and not item.strip().startswith("#") and len(item) - len(item.lstrip()) <= indent:
                    break
            break
    return out


def read_table():
    rows = {}
    if TABLE.exists():
        for number, line in enumerate(TABLE.read_text(encoding="utf-8").splitlines()):
            if number == 0 or not line.strip():
                continue
            cells = line.split("\t")
            cells += [""] * (len(HEADER) - len(cells))
            rows[cells[0]] = cells[: len(HEADER)]
    return rows


def new_row(name, importers):
    if name in SEED:
        status, source = SEED[name]
        sample = name
        licence = OURS
    elif name.startswith("common/"):
        status, source, sample, licence = "part-of", "-", "-", "-"
    else:
        status, source, sample, licence = "none", "-", "-", "-"
    note = NOTES.get(name, "")
    if status == "part-of":
        users = sorted(importers.get(name, []))
        if users:
            note = "imported by " + ", ".join(users)
        else:
            # a definition made to be imported, but that nothing in the gallery imports
            status = "none"
            note = "made to be imported by other definitions; none of the gallery does"
    return [name, status, sample, source, licence, note]


def mark(arguments):
    """--mark STATUS SOURCE NOTE FORMAT...: the status, source and licence of the rows
    of formats that have samples; the note is kept when NOTE is -."""
    status, source, note, *names = arguments
    if status not in STATUSES - {"none", "part-of"} or not names:
        sys.exit("usage: --mark real|crafted|failing SOURCE NOTE FORMAT...")
    rows = read_table()
    for name in names:
        if name not in rows:
            sys.exit(f"{name} is not a definition of the gallery")
        if not any((HERE / "samples" / "gallery" / name).glob("*.bin")):
            sys.exit(f"{name} has no samples in samples/gallery/{name}")
        row = rows[name]
        row[1], row[2], row[3], row[4] = status, name, source, OURS
        if note != "-":
            row[5] = note
    lines = ["\t".join(HEADER)] + ["\t".join(rows[name]) for name in sorted(rows)]
    TABLE.write_text("\n".join(lines) + "\n", encoding="utf-8")
    print(f"{len(names)} rows marked {status}")


def readme_block(rows):
    """The table of directories and the formats that are tested, as the README shows it."""
    by_dir = {}
    for name, cells in rows.items():
        if cells[1] in ("real", "crafted"):
            directory, _, leaf = name.partition("/")
            by_dir.setdefault(directory, []).append(leaf)
    lines = ["<!-- gallery-formats:begin -->", "| Directory | Formats |", "| --- | --- |"]
    for directory in sorted(by_dir):
        formats = ", ".join(f"`{leaf}`" for leaf in sorted(by_dir[directory]))
        lines.append(f"| `{directory}` | {formats} |")
    lines.append("<!-- gallery-formats:end -->")
    return "\n".join(lines)


def update_readme():
    readme = HERE.parent / "README.md"
    text = readme.read_text(encoding="utf-8")
    begin = text.index("<!-- gallery-formats:begin -->")
    end = text.index("<!-- gallery-formats:end -->") + len("<!-- gallery-formats:end -->")
    readme.write_text(text[:begin] + readme_block(read_table()) + text[end:], encoding="utf-8")


def update_tests():
    """Write the list of tests of conformance.rs: one for each row that is real or crafted."""
    path = HERE.parent / "crates" / "doe" / "tests" / "integration" / "conformance.rs"
    text = path.read_text(encoding="utf-8")
    start = text.index("gallery!(\n")
    end = text.index("\n);", start)
    lines = ["gallery!("]
    for name, cells in sorted(read_table().items()):
        if cells[1] in ("real", "crafted"):
            lines.append(f'    gallery_{name.replace("/", "_")} = "gallery/{name}",')
    path.write_text(text[:start] + "\n".join(lines) + text[end:], encoding="utf-8")
    print(f"{len(lines) - 1} tests")


def main():
    if sys.argv[1:2] == ["--tests"]:
        update_tests()
        return
    if sys.argv[1:2] == ["--readme"]:
        update_readme()
        return
    if sys.argv[1:2] == ["--mark"]:
        mark(sys.argv[2:])
        return
    check = "--check" in sys.argv[1:]
    names = definitions()
    if not names:
        sys.exit("the submodule conformance/kaitai_struct_formats is not checked out")
    importers = {}
    for name in names:
        for target in imports_of(name):
            importers.setdefault(target, set()).add(name)
    rows = read_table()
    added = [n for n in names if n not in rows]
    removed = [n for n in rows if n not in names]
    if check:
        if added or removed:
            sys.exit(f"the table is out of date: {len(added)} new definitions, {len(removed)} removed")
        return
    for name in added:
        rows[name] = new_row(name, importers)
    for name in removed:
        del rows[name]
    lines = ["\t".join(HEADER)] + ["\t".join(rows[name]) for name in sorted(rows)]
    TABLE.write_text("\n".join(lines) + "\n", encoding="utf-8")
    print(f"{len(rows)} rows: {len(added)} added, {len(removed)} removed")


if __name__ == "__main__":
    main()
