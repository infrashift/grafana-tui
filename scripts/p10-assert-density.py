#!/usr/bin/env python3
"""Check the assertion density of the production Rust code (Power of 10 rule R5).

ADR-0002 counts the non-trivial functions only. A function is non-trivial
when it has more than MIN_FN_LINES lines, from the `fn` line to the closing
brace. The check fails when the average of assertions in the non-trivial
functions is less than MIN_AVERAGE.

Assertions: assert!, assert_eq!, assert_ne!, debug_assert!,
debug_assert_eq!, debug_assert_ne!, and the anyhow macros ensure! and bail!,
which return an error to the caller (ADR-0002).

The script skips `#[cfg(test)] mod` blocks. An assertion counts for the
innermost function that holds its line.

Run from the repository root: python3 -I scripts/p10-assert-density.py [--verbose]
With --verbose, the script lists each non-trivial function with fewer than
MIN_AVERAGE assertions, and the count for each file.
"""
import pathlib
import re
import sys

MIN_FN_LINES = 5
MIN_AVERAGE = 2.0
FN_RE = re.compile(r"^\s*(pub(\([^)]*\))?\s+)?(const\s+)?(async\s+)?(unsafe\s+)?fn\s+(\w+)")
ASSERT_RE = re.compile(
    r"(?<![\w!])(?:debug_)?assert(?:_eq|_ne)?!|(?<![\w!])(?:anyhow::)?(?:ensure|bail)!"
)
TEST_MOD_RE = re.compile(r"^\s*mod\s+\w+")
def blank_literals(text):
    """Return `text` with comments and literals blanked. Newlines stay.

    The lexer knows line and block comments, strings, byte strings, raw
    strings (`r#"..."#`) and char literals. It keeps a lifetime (`'a`) as code.
    The loop moves forward at least one character each pass, so its bound is
    the length of the text.
    """
    out = []
    i, n = 0, len(text)
    while i < n:
        c = text[i]
        two = text[i:i + 2]
        if two == "//":
            j = text.find("\n", i)
            j = n if j < 0 else j
            out.append(" " * (j - i))
            i = j
        elif two == "/*":
            j = text.find("*/", i + 2)
            j = n if j < 0 else j + 2
            out.append(re.sub(r"[^\n]", " ", text[i:j]))
            i = j
        elif c == "r" and re.match(r'r#*"', text[i:]) and (i == 0 or not (text[i - 1].isalnum() or text[i - 1] == "_")):
            hashes = len(re.match(r"r(#*)", text[i:]).group(1))
            close = '"' + "#" * hashes
            start = i + 2 + hashes
            j = text.find(close, start)
            j = n if j < 0 else j + len(close)
            out.append('""' + re.sub(r"[^\n]", " ", text[i + 2:j]))
            i = j
        elif c == '"':
            j = i + 1
            while j < n and text[j] != '"':
                j += 2 if text[j] == "\\" else 1
            j = min(j + 1, n)
            out.append('""' + re.sub(r"[^\n]", " ", text[i + 2:j]) if j - i >= 2 else '"')
            i = j
        elif c == "'":
            m = re.match(r"'(?:\\.[^']*|[^'\\])'", text[i:])
            if m:
                out.append("''" + " " * (len(m.group(0)) - 2))
                i += len(m.group(0))
            else:
                out.append(c)
                i += 1
        else:
            out.append(c)
            i += 1
    return "".join(out)


def production_lines(text):
    """Return the code lines of a file, with each `#[cfg(test)] mod` block blanked."""
    lines = blank_literals(text).split("\n")
    out = list(lines)
    i = 0
    while i < len(lines):
        if lines[i].strip() == "#[cfg(test)]" and i + 1 < len(lines) and TEST_MOD_RE.match(lines[i + 1]):
            depth, started, j = 0, False, i + 1
            while j < len(lines):
                depth += lines[j].count("{") - lines[j].count("}")
                started = started or "{" in lines[j]
                out[j] = ""
                if started and depth <= 0:
                    break
                j += 1
            out[i] = ""
            i = j
        i += 1
    return out


def functions(lines):
    """Yield (name, first line, last line) for each function with a body."""
    for i, line in enumerate(lines):
        match = FN_RE.match(line)
        if not match:
            continue
        depth, started = 0, False
        for j in range(i, len(lines)):
            code = lines[j]
            if not started and ";" in code and "{" not in code:
                break
            depth += code.count("{") - code.count("}")
            started = started or "{" in code
            if started and depth <= 0:
                yield match.group(6), i, j
                break


def scan(root):
    """Return a list of (path, name, line, lines, assertions) for the functions."""
    result = []
    for path in sorted(root.glob("src/**/*.rs")):
        lines = production_lines(path.read_text())
        spans = list(functions(lines))
        owner = {}
        for index, (_, first, last) in enumerate(spans):
            for n in range(first, last + 1):
                owner[n] = index  # a later (inner) function overwrites the outer one
        counts = [0] * len(spans)
        for n, line in enumerate(lines):
            if n in owner:
                counts[owner[n]] += len(ASSERT_RE.findall(line))
        for index, (name, first, last) in enumerate(spans):
            rel = path.relative_to(root)
            result.append((str(rel), name, first + 1, last - first + 1, counts[index]))
    return result


def main():
    root = pathlib.Path.cwd()
    verbose = "--verbose" in sys.argv[1:]
    found = [f for f in scan(root) if f[3] > MIN_FN_LINES]
    total = sum(f[4] for f in found)
    average = total / len(found) if found else 0.0
    if verbose:
        per_file = {}
        for path, name, line, size, count in found:
            need = per_file.setdefault(path, [0, 0])
            need[0] += 1
            need[1] += count
            if count < MIN_AVERAGE:
                print(f"{path}:{line}: `{name}` ({size} lines) has {count} assertion(s)")
        print("--- per file: functions, assertions, missing for the average")
        for path, (fns, count) in sorted(per_file.items()):
            missing = max(0, int(MIN_AVERAGE * fns) - count)
            print(f"{path}: {fns} {count} {missing}")
    print(
        f"p10-assert-density: {total} assertions in {len(found)} functions of more than "
        f"{MIN_FN_LINES} lines (average {average:.2f}, min {MIN_AVERAGE})"
    )
    return 0 if average >= MIN_AVERAGE else 1


if __name__ == "__main__":
    sys.exit(main())
