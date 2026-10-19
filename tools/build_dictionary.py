"""Build a bounded, read-only English-Chinese dictionary (Python standard library only)."""

import argparse
from contextlib import closing
import csv
import hashlib
import io
from pathlib import Path
import re
import sqlite3
import tempfile
import urllib.request

COMMIT = "bc015ed2e24a7abef49fc6dbbb7fe32c1dadaf8b"
URL = f"https://raw.githubusercontent.com/skywind3000/ECDICT/{COMMIT}/ecdict.csv"


def build(path, rows):
    path.unlink(missing_ok=True)
    with closing(sqlite3.connect(path)) as db:
        db.execute("PRAGMA page_size=4096")
        db.execute("CREATE TABLE words(word TEXT PRIMARY KEY, meaning TEXT NOT NULL) WITHOUT ROWID")
        db.executemany("INSERT INTO words VALUES (?, ?)", sorted(rows))
        db.commit()
        db.execute("VACUUM")
    return path.stat().st_size


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=Path("data") / "dictionary.db")
    parser.add_argument("--max-bytes", type=int, default=10_000_000)
    args = parser.parse_args()
    with urllib.request.urlopen(URL, timeout=120) as response:
        source = response.read()
    # Pin both the revision and Git blob identity of the upstream input.
    digest = hashlib.sha1(f"blob {len(source)}\0".encode() + source).hexdigest()
    if digest != "c4ade63ea08cf39d9c3475e96929036d64d94c94":
        raise RuntimeError("Dictionary source checksum mismatch")
    entries = {}
    for row in csv.DictReader(io.StringIO(source.decode("utf-8-sig"))):
        word = row["word"].strip().lower()
        meaning = row["translation"].replace("\\n", "\n").strip()
        if not re.fullmatch(r"[a-z][a-z'-]{0,79}", word) or not re.search(r"[\u3400-\u9fff]", meaning):
            continue
        # Drop English part-of-speech labels; keep the Chinese meanings themselves.
        meaning = re.sub(r"(?m)^(?:[a-z]+\. ?)+", "", meaning).strip()
        ranks = [int(row[key]) for key in ("bnc", "frq") if row.get(key, "").isdigit() and int(row[key]) > 0]
        rank = min(ranks) if ranks else 1_000_000
        if word not in entries or rank < entries[word][0]:
            entries[word] = (rank, meaning)
    ordered = [(word, item[1]) for word, item in sorted(entries.items(), key=lambda pair: (pair[1][0], pair[0]))]
    args.output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory() as temp:
        candidate = Path(temp) / "dictionary.db"
        low, high = 0, len(ordered)
        while low < high:
            middle = (low + high + 1) // 2
            if build(candidate, ordered[:middle]) <= args.max_bytes:
                low = middle
            else:
                high = middle - 1
        if low == 0:
            raise RuntimeError("Dictionary budget is too small")
        size = build(candidate, ordered[:low])
        args.output.write_bytes(candidate.read_bytes())
    with closing(sqlite3.connect(f"{args.output.resolve().as_uri()}?mode=ro", uri=True)) as db:
        if db.execute("PRAGMA integrity_check").fetchone()[0] != "ok":
            raise RuntimeError("Dictionary integrity check failed")
    print(f"{low:,} entries; {size:,} bytes; {args.output}")


if __name__ == "__main__":
    main()
