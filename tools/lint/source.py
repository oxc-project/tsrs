#!/usr/bin/env python3
"""Source checks that clippy cannot express (docs/RUST.md, "Source checks").

  tools/lint/source.py             check; exit 1 on a change to the Send/Sync or unsafe-site inventories or a new
                                   uncommented ordering
  tools/lint/source.py --update    rewrite the inventories and lower the ordering counts (after reviewing the change)
  tools/lint/source.py --update --allow-increase   also raise ordering counts (first run, or a deliberate exception)
  tools/lint/source.py --self-test check the patterns on fixed snippets

1. `unsafe impl Send` / `unsafe impl Sync`: tools/lint/send_sync.tsv lists every one. Adding or removing one fails
   until --update records it, so each new cross-thread escape is a reviewed line in a diff (P<T> is Send + Sync by
   decree; docs/PORTING.md, "Threading"). After Bun's vm-thread-door inventory.
2. Weakened atomic orderings (Relaxed, Acquire, Release, AcqRel) need a `//` comment on the same line or in the three
   lines above saying why the ordering is enough. tools/lint/atomics.tsv holds the per-file count of those without
   one; a file may not go above it. Bun's rule: "default to seq_cst and comment any weakened ordering".
3. An arena free (`free!`, `free_slice!`, `free_raw`, `free_slice_ptr`) of a function's own reference parameter
   (`x: &T`, `x: &[T]`, or `self` in a `&self` method). A reference parameter is a protected borrow for the whole call
   (LLVM `noalias`), so writing the free-list link into its memory is undefined behaviour; LLVM deleted that write in
   `panic = "abort"` builds (notes/fix-arena-recycle-uaf.md). Take the block as a `P<T>`, an address or a raw slice.
   No baseline: every finding fails.
4. Every `unsafe` site and lifetime escape: tools/lint/unsafe_sites.tsv lists each `unsafe` block, `unsafe fn`,
   `unsafe impl`, `unsafe extern`, `Box::leak`, `Box::from_raw`, `mem::forget`, `transmute` and `assume_static`, keyed
   by file, enclosing item, kind and its SAFETY line (not by line number, so moving code does not churn it), with a
   class (boundary, core-allocator, ownership, logic, test), a verdict (keep, safe-api-possible, defect-risk, unknown) and a
   note that --update preserves. A site that appears or disappears fails until --update records it, so every new
   `unsafe` is a reviewed line with a stated class and verdict (notes/unsafe-audit.md, issue #646).

Scans {crates,tasks}/*/src/**/*.rs, without test files (*_test.rs, tests.rs, tests/ directories)
and files whose first line says they are generated.
"""

import os
import re
import sys
from pathlib import Path

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
INVENTORY = os.path.join(ROOT, "tools", "lint", "send_sync.tsv")
ATOMICS = os.path.join(ROOT, "tools", "lint", "atomics.tsv")
UNSAFE_SITES = os.path.join(ROOT, "tools", "lint", "unsafe_sites.tsv")
UNSAFE_HEADER = ("# Every unsafe site and lifetime escape (tools/lint/source.py --update; notes/unsafe-audit.md).\n"
                 "# file\titem\tkind\tclass\tsafety\tverdict\tnote. --update keeps class, verdict and note of a row whose key\n"
                 "# (file, item, kind, safety) still exists; a new site gets a class from its crate and verdict `unknown`.\n")

SEND_SYNC = re.compile(r"\bunsafe\s+impl\b(?:\s*<(?P<generics>[^{]*?)>)?\s+(?P<trait>Send|Sync)\s+for\s+(?P<type>[^{]+?)\s*\{")
WEAK = ("Relaxed", "Acquire", "Release", "AcqRel")
QUALIFIED = re.compile(r"\bOrdering::(Relaxed|Acquire|Release|AcqRel)\b")
IMPORTED = re.compile(r"\buse\s+std::sync::atomic::(?:\{[^}]*\bOrdering::(?:\{[^}]*\}|\w+)[^}]*\}|Ordering::(?:\{[^}]*\}|\w+))\s*;")
LOOKBACK = 3
FN_START = re.compile(r"\bfn\s+\w+\s*(?:<[^(]*?>)?\s*\(")
FREE_CALL = re.compile(r"\b(free!|free_slice!|free_raw|free_slice_ptr)\s*\(\s*&?\s*(\w+)")


def source_files():
    sources = sorted(Path(ROOT).glob("crates/*/src")) + sorted(Path(ROOT).glob("tasks/*/src"))
    for source in sources:
        src = str(source)
        for dirpath, _dirs, files in os.walk(src):
            if os.sep + "tests" in dirpath[len(src):]:
                continue
            for name in sorted(files):
                if name.endswith(".rs") and not name.endswith(("_test.rs", "_tests.rs")) and name != "tests.rs":
                    path = os.path.join(dirpath, name)
                    text = open(path, encoding="utf-8", errors="replace").read()
                    first = text.split("\n", 1)[0]
                    if "Code generated" in first or "@generated" in first:
                        continue
                    yield os.path.relpath(path, ROOT), text


def code_part(line):
    """The line without a trailing `//` comment (string literals with `//` are rare enough here to ignore)."""
    i = line.find("//")
    return line if i < 0 else line[:i]


def send_sync_impls(rel, text):
    rows = []
    for m in SEND_SYNC.finditer(text):
        line_start = text.rfind("\n", 0, m.start()) + 1
        if text[line_start:m.start()].lstrip().startswith("//"):
            continue
        generics = f"<{m.group('generics').strip()}> " if m.group("generics") else ""
        rows.append((rel, m.group("trait"), generics + " ".join(m.group("type").split())))
    return rows


def imported_orderings(text):
    names = set()
    for m in IMPORTED.finditer(text):
        names.update(w for w in WEAK if re.search(r"\b" + w + r"\b", m.group(0)))
    return names


def uncommented_orderings(text):
    """(line number, line) of each weakened ordering without a nearby comment."""
    bare = imported_orderings(text)
    bare_re = re.compile(r"(?<![\w:])(" + "|".join(sorted(bare)) + r")\b") if bare else None
    lines = text.split("\n")
    found = []
    for i, line in enumerate(lines):
        code = code_part(line)
        if code.lstrip().startswith("use "):
            continue
        hits = len(QUALIFIED.findall(code)) + (len(bare_re.findall(code)) if bare_re else 0)
        if not hits:
            continue
        commented = "//" in line or any("//" in lines[j] for j in range(max(0, i - LOOKBACK), i))
        if not commented:
            found.extend([(i + 1, line.strip())] * hits)
    return found


def matching(text, i, open_ch, close_ch):
    """Index just past the bracket that closes the one at text[i] (strings and comments are not special-cased)."""
    depth = 0
    for j in range(i, len(text)):
        if text[j] == open_ch:
            depth += 1
        elif text[j] == close_ch:
            depth -= 1
            if depth == 0:
                return j + 1
    return len(text)


def reference_params(params):
    """Names of the parameters whose type is a reference (`&self` counts as `self`)."""
    names, depth, cur = set(), 0, ""
    for ch in params + ",":
        if ch in "<([":
            depth += 1
        elif ch in ">)]":
            depth -= 1
        if ch == "," and depth == 0:
            p = cur.strip()
            cur = ""
            if re.match(r"^&\s*(?:'\w+\s+)?(?:mut\s+)?self$", p):
                names.add("self")
            m = re.match(r"^(?:mut\s+)?(\w+)\s*:\s*&", p)
            if m:
                names.add(m.group(1))
        else:
            cur += ch
    return names


def frees_of_reference_params(text):
    """(line number, line) of each arena free whose argument is a reference parameter of the enclosing function."""
    code = "\n".join(code_part(l) for l in text.split("\n"))
    found = []
    for m in FN_START.finditer(code):
        open_paren = m.end() - 1
        close = matching(code, open_paren, "(", ")")
        brace = code.find("{", close)
        semi = code.find(";", close)
        if brace < 0 or (0 <= semi < brace):
            continue  # a declaration without a body
        refs = reference_params(code[open_paren + 1:close - 1])
        if not refs:
            continue
        body_end = matching(code, brace, "{", "}")
        for f in FREE_CALL.finditer(code, brace, body_end):
            if f.group(2) in refs:
                line_no = code.count("\n", 0, f.start()) + 1
                line = text.split("\n")[line_no - 1]
                if "source.py: deliberate" not in line:  # the Miri negative control in tsrs_core's arena tests
                    found.append((line_no, line.strip()))
    return found


# --- 4. the unsafe inventory -------------------------------------------------------------------------------------

SITE = re.compile(r"\bunsafe\s*\{|\bunsafe\s+fn\s+(?P<fn>\w+)|\bunsafe\s+impl\b|\bunsafe\s+extern\b|Box::leak\b|"
                  r"Box::from_raw\b|mem::forget\b|\btransmute\b|\bassume_static\b")
SITE_WORDS = ("unsafe", "Box::leak", "Box::from_raw", "mem::forget", "transmute", "assume_static")
SAFETY = re.compile(r"SAFETY:?\s*(.*)")
REASON = re.compile(r'reason\s*=\s*"([^"]*)"')
KINDS = {"{": "block", "i": "impl", "e": "extern", "Box::leak": "leak", "Box::from_raw": "from_raw",
         "mem::forget": "forget", "transmute": "transmute", "assume_static": "lifetime"}
CLASSES = ("boundary", "core-allocator", "ownership", "logic", "test")
VERDICTS = ("keep", "safe-api-possible", "defect-risk", "unknown")
CORE_ALLOCATOR = ("ptr.rs", "arena.rs", "reserve.rs", "memsplit.rs", "sharedgraph.rs", "frozen.rs")
BOUNDARY_CRATES = ("tsrs_fswatch", "tsrs_wasm", "tsrs_cli")
OWNERSHIP_KINDS = ("leak", "from_raw", "forget", "lifetime")
OWNERSHIP_CALLS = re.compile(r"free_program|free_unshared_program|SharedProgramData|\.free\(\)|free_checkers|"
                             r"free_project_reference_file_mapper|free_api_orchestrator|drop_replaced|free_replaced|"
                             r"Box::from_raw|Box::leak")
OWNERSHIP_ITEMS = re.compile(r"^(assume_static$|free_|SharedProgramData::|Program::(drop|free)_replaced$)")


def matching_code(code, i, open_ch, close_ch):
    """`matching`, skipping string and char literals (a `"{"` in a format string must not unbalance an item)."""
    depth, j, n = 0, i, len(code)
    while j < n:
        ch = code[j]
        if ch == '"':
            j += 1
            while j < n and code[j] != '"':
                j += 2 if code[j] == "\\" else 1
        elif ch == "'" and j + 2 < n and (code[j + 2] == "'" or (code[j + 1] == "\\" and code[j + 3:j + 4] == "'")):
            j = code.find("'", j + 2)
        elif ch == open_ch:
            depth += 1
        elif ch == close_ch:
            depth -= 1
            if depth == 0:
                return j + 1
        j += 1
    return n


ITEM_HEAD = (r"(?:^|[\s;}])(?:pub(?:\([^)]*\))?\s+)?(?:(?:const|async|unsafe|extern\s+\"[^\"]*\")\s+)*fn\s+(?P<fn>\w+)"
             r"|^[ \t]*(?:unsafe\s+)?impl\s*(?:<[^{=]*?>\s*)?(?P<impl>[^{;]*?)\s*\{"
             r"|(?:^|[\s;}])(?:pub(?:\([^)]*\))?\s+)?static\s+(?:mut\s+)?(?P<static>\w+)\s*:"
             r"|(?:^|[\s;}])macro_rules!\s+(?P<macro>\w+)"
             r"|#\[cfg\(test\)\]\s*(?:pub(?:\([^)]*\))?\s+)?mod\s+(?P<testmod>\w+)\s*\{")
LITERAL = r'\br(?P<rh>#*)"[\s\S]*?"(?P=rh)|b?"(?:[^"\\]|\\[\s\S])*"|b?\'(?:[^\'\\]|\\.)\''
TOKEN = re.compile(LITERAL + r"|[{}()\[\];]|" + ITEM_HEAD, re.M)
COMMENTS = re.compile(r"(" + LITERAL + r")|//[^\n]*|/\*[\s\S]*?\*/")


def strip_comments(text):
    """The text with comments blanked (newlines kept, so offsets and line numbers hold); literals are left alone."""
    return COMMENTS.sub(lambda m: m.group(1) if m.group(1) else "\n" * m.group(0).count("\n"), text)


def items(code):
    """(start, end, label) of every fn, impl, static, macro_rules and `#[cfg(test)] mod` item, in one token pass.

    A declaration (trait method, extern fn) ends at its `;`; an item with a body ends after its closing brace.
    Strings and char literals are skipped, so a `"{"` cannot unbalance an item."""
    out, stack, depth, pending, nest = [], [], 0, None, 0
    for m in TOKEN.finditer(code):
        t = m.group(0)
        if m.lastgroup in ("fn", "static", "macro"):
            if pending:
                out.append((pending[1], m.start(), pending[0]))
            pending, nest = (("fn " if m.lastgroup == "fn" else m.lastgroup + " ") + m.group(m.lastgroup), m.start()), 0
        elif m.lastgroup in ("impl", "testmod"):
            if pending:
                out.append((pending[1], m.start(), pending[0]))
                pending = None
            label = ("impl " + re.sub(r"\s*\bwhere\b.*$", "", " ".join(m.group("impl").split()))) if m.lastgroup == "impl" \
                else "testmod " + m.group("testmod")
            depth += 1
            stack.append((label, depth, m.start()))
        elif t == "{":
            depth += 1
            if pending and nest == 0:
                stack.append((pending[0], depth, pending[1]))
                pending = None
        elif t == "}":
            if stack and stack[-1][1] == depth:
                label, _d, start = stack.pop()
                out.append((start, m.end(), label))
            depth -= 1
        elif t in "([":
            nest += 1
        elif t in ")]":
            nest -= 1
        elif t == ";" and pending and nest == 0:
            out.append((pending[1], m.start(), pending[0]))
            pending = None
    for label, _d, start in stack:
        out.append((start, len(code), label))
    return out


def test_ranges(all_items):
    return [(start, end) for start, end, label in all_items if label.startswith("testmod ")]


def enclosing(all_items, pos):
    """`Type::method`, `Trait for Type`, a free `fn`, `static X`, `macro m` of the innermost item at pos; `(file)` if none."""
    inner = sorted((it for it in all_items if it[0] <= pos < it[1] and not it[2].startswith("testmod ")),
                   key=lambda it: it[1] - it[0])
    if not inner:
        return "(file)"
    label = inner[0][2]
    if label.startswith("fn "):
        impls = [it for it in inner[1:] if it[2].startswith("impl ")]
        if impls:
            typ = min(impls, key=lambda it: it[1] - it[0])[2][5:].split(" for ")[-1]
            return f"{re.sub(r'<.*', '', typ).strip()}::{label[3:]}"
        return label[3:]
    return label


def site_kind(m):
    text = m.group(0)
    if m.group("fn"):
        return "fn"
    return KINDS[text[6:].lstrip()[0] if text.startswith("unsafe") else text]


def statement_lines_above(lines, i, limit):
    """Line indexes i, i-1, ... while still inside the statement or attribute/doc block that ends at line i."""
    yield i
    for j in range(i - 1, max(-1, i - limit), -1):
        code = code_part(lines[j]).rstrip()
        stripped = lines[j].strip()
        if not stripped:
            return
        if code.endswith((";", "{", "}")) and not stripped.startswith(("//", "#[", "#![")):
            return
        yield j


def comment_above(lines, i):
    """The SAFETY comment or `reason = "..."` nearest above (or on) line i, normalized; "" if none."""
    for j in statement_lines_above(lines, i, 10):
        m = SAFETY.search(lines[j])
        if m:
            text = m.group(1).strip()
            if not text and j + 1 < len(lines) and lines[j + 1].strip().startswith("//"):
                text = lines[j + 1].strip().lstrip("/").strip()
            return " ".join(text.split())[:110]
        m = REASON.search(lines[j])
        if m:
            return " ".join(m.group(1).split())[:110]
    return ""


def doc_safety(lines, i):
    """The first line of the `# Safety` doc section of the fn declared at line i, "" if none."""
    for j in statement_lines_above(lines, i, 24):
        if j < i and "# Safety" in lines[j]:
            for k in range(j + 1, i):
                if lines[k].strip().startswith("///") and lines[k].strip("/ \t"):
                    return " ".join(lines[k].strip().lstrip("/").split())[:110]
    return ""


def unsafe_sites(rel, text):
    """(file, item, kind, safety, class) of every unsafe site and lifetime escape, in source order."""
    if not any(w in text for w in SITE_WORDS):
        return []
    lines = text.split("\n")
    code = strip_comments(text)
    all_items = items(code)
    tests = test_ranges(all_items)
    rows = []
    for m in SITE.finditer(code):
        kind = site_kind(m)
        line_no = code.count("\n", 0, m.start())
        line = lines[line_no]
        if line.lstrip().startswith("use ") or (kind == "transmute" and "fn transmute" in line):
            continue
        if kind == "lifetime" and "fn assume_static" in line:
            item = "assume_static"
        else:
            item = enclosing(all_items, m.start())
        safety = doc_safety(lines, line_no) if kind == "fn" else comment_above(lines, line_no)
        body = code[m.start():matching_code(code, code.find("{", m.start()), "{", "}")] if kind == "block" else ""
        in_test = any(a <= m.start() < b for a, b in tests)
        rows.append((rel, item, kind, safety, "test" if in_test else classify(rel, item, kind, body)))
    return rows


def classify(rel, item, kind, body):
    """The default class of a new site; the committed TSV's class wins once a row exists."""
    crate = rel.split("/")[1] if rel.startswith(("crates/", "tasks/")) else ""
    name = os.path.basename(rel)
    if crate == "tsrs_core" and name in CORE_ALLOCATOR:
        return "core-allocator"
    if kind == "extern":
        return "boundary"
    if kind in OWNERSHIP_KINDS or OWNERSHIP_CALLS.search(body) or OWNERSHIP_ITEMS.search(item):
        return "ownership"
    if crate in BOUNDARY_CRATES or name == "allocator.rs":
        return "boundary"
    return "logic"


def unsafe_inventory():
    """{(file, item, kind, safety): class} for every site; a repeated key gets a ` #n` suffix on its safety."""
    rows, seen = {}, {}
    for rel, text in source_files():
        for f, item, kind, safety, cls in unsafe_sites(rel, text):
            key = (f, item, kind, safety)
            n = seen[key] = seen.get(key, 0) + 1
            if n > 1:
                key = (f, item, kind, f"{safety} #{n}".strip())
            rows[key] = cls
    return rows


def read_unsafe_tsv():
    """{(file, item, kind, safety): (class, verdict, note)} as committed."""
    out = {}
    for row in read_tsv(UNSAFE_SITES):
        row = tuple(row) + ("",) * (7 - len(row))
        out[tuple(row[:3]) + (row[4],)] = (row[3], row[5], row[6])
    return out


def merge_unsafe(current, recorded):
    """The rows to write: current keys, with the recorded class, verdict and note where the key still exists."""
    rows = []
    for key in sorted(current):
        cls, verdict, note = recorded.get(key, (current[key], "unknown", ""))
        rows.append((key[0], key[1], key[2], cls, key[3], verdict, note))
    return rows


def scan():
    inventory, atomics, details, files = [], {}, {}, 0
    global REF_FREES
    REF_FREES = []
    for rel, text in source_files():
        files += 1
        inventory.extend(send_sync_impls(rel, text))
        REF_FREES.extend((rel, n, l) for n, l in frees_of_reference_params(text))
        found = uncommented_orderings(text)
        if found:
            atomics[rel] = len(found)
            details[rel] = found
    return files, sorted(inventory), atomics, details


def unsafe_report(current, recorded):
    """Lines describing sites that appeared, disappeared or carry an invalid class or verdict; empty when in sync."""
    lines = []
    for key in sorted(set(current) - set(recorded)):
        lines.append(f"new unsafe site: {key[0]} {key[1]} ({key[2]}): state its class and verdict in {os.path.relpath(UNSAFE_SITES, ROOT)} "
                     f"after tools/lint/source.py --update (it is recorded as `{current[key]}` / `unknown` until then)")
    for key in sorted(set(recorded) - set(current)):
        lines.append(f"unsafe site gone: {key[0]} {key[1]} ({key[2]}): record it with tools/lint/source.py --update")
    for key, (cls, verdict, _note) in sorted(recorded.items()):
        if cls not in CLASSES or verdict not in VERDICTS:
            lines.append(f"unsafe site {key[0]} {key[1]} ({key[2]}): class `{cls}` / verdict `{verdict}` is not one of "
                         f"{'/'.join(CLASSES)} and {'/'.join(VERDICTS)}")
    return lines


def read_tsv(path):
    if not os.path.exists(path):
        return []
    return [tuple(l.rstrip("\n").split("\t")) for l in open(path) if l.strip() and not l.startswith("#")]


def write(path, header, rows):
    with open(path, "w") as f:
        f.write(header)
        for row in rows:
            f.write("\t".join(str(x) for x in row) + "\n")


def self_test():
    rows = send_sync_impls("x.rs", "unsafe impl Send for A {}\nunsafe impl<T: ?Sized> Sync for P<T> {}\n// unsafe impl Send for C {}\n")
    assert rows == [("x.rs", "Send", "A"), ("x.rs", "Sync", "<T: ?Sized> P<T>")], rows
    assert send_sync_impls("x.rs", "impl Send for Safe {}\n") == []
    text = "\n".join([
        "use std::sync::atomic::{AtomicU32, Ordering::Relaxed};",
        "fn f() {",
        "    a.load(Ordering::Relaxed);",            # flagged
        "    b.load(Relaxed); // counter only",       # comment on the line
        "    // ids only need to be unique",
        "    c.fetch_add(1, Relaxed);",               # comment above
        "    d.store(1, Ordering::SeqCst);",          # not weakened
        "    let x = 1;",
        "    e.compare_exchange(0, 1, Ordering::Acquire, Ordering::Relaxed);",  # two; the nearest comment is 4 lines up
        "}",
    ])
    found = uncommented_orderings(text)
    assert [n for n, _ in found] == [3, 9, 9], found
    assert uncommented_orderings("enum Kind { Release }\nlet k = Kind::Release;\n") == []
    bad = "\n".join([
        "unsafe fn recycle(m: P<TypeMapper>, targets: &'static [P<Type>]) {",
        "    tsrs_core::free!(m);",                    # by value: fine
        "    tsrs_core::free_slice!(targets);",        # flagged
        "}",
        "fn ok(targets: *const [P<Type>], n: usize) { unsafe { tsrs_core::free_slice_ptr(targets) } }",
        "impl X { fn drop_me(&self) { unsafe { free_raw(self as *const X as usize, 8, 8, false) } } }",  # flagged
        "fn local() { let s: &[u8] = alloc_slice_recycled(&[1]); unsafe { free_slice!(s) } }",
        "fn decl(s: &[u8]);",
        "fn after(x: u32) { free!(p) }",
    ])
    assert [n for n, _ in frees_of_reference_params(bad)] == [3, 6], frees_of_reference_params(bad)
    src = "\n".join([
        "impl Store {",
        "    /// # Safety",
        "    /// `i` is below `len`.",
        "    #[inline]",
        "    pub unsafe fn at(&self, i: usize) -> u32 {",
        "        // SAFETY: the caller's contract.",
        "        unsafe { *self.p.add(i) }",
        "    }",
        "    fn out(&self) -> [u64; 2] {",   # a `;` inside the return type is not a declaration
        "        // SAFETY: a plain (the key keeps the first line)",
        "        // byte copy.",
        "        let w = unsafe { std::mem::transmute::<[u8; 16], [u64; 2]>(self.b) };",
        "        w",
        "    }",
        "}",
        "fn leak(h: Host) -> &'static Host {",
        "    #[expect(clippy::disallowed_methods, reason = \"lives for the process\")]",
        "    Box::leak(Box::new(h))",
        "}",
        "fn fmt(s: &str) -> String { format!(\"{{\") }",  # an unbalanced brace in a string
        "unsafe extern \"C\" { fn malloc_trim(pad: i32) -> i32; }",
        "#[cfg(test)]",
        "mod tests {",
        "    fn t() { unsafe { touch() } }",
        "}",
    ])
    rows = unsafe_sites("crates/x/src/x.rs", src)
    assert rows == [
        ("crates/x/src/x.rs", "Store::at", "fn", "`i` is below `len`.", "logic"),
        ("crates/x/src/x.rs", "Store::at", "block", "the caller's contract.", "logic"),
        ("crates/x/src/x.rs", "Store::out", "block", "a plain (the key keeps the first line)", "logic"),
        ("crates/x/src/x.rs", "Store::out", "transmute", "a plain (the key keeps the first line)", "logic"),
        ("crates/x/src/x.rs", "leak", "leak", "lives for the process", "ownership"),
        ("crates/x/src/x.rs", "(file)", "extern", "", "boundary"),
        ("crates/x/src/x.rs", "t", "block", "", "test"),
    ], rows
    assert classify("crates/tsrs_core/src/arena.rs", "free_block", "fn", "") == "core-allocator"
    assert classify("crates/tsrs_fswatch/src/kqueue.rs", "close_fd", "block", "{ libc::close(fd) }") == "boundary"
    assert classify("crates/tsrs_execute/src/watcher.rs", "Watcher::install_program", "block", "{ free_program(old) }") == "ownership"
    current = {("a.rs", "f", "block", "x"): "logic", ("a.rs", "g", "block", "y"): "logic"}
    recorded = {("a.rs", "f", "block", "x"): ("boundary", "keep", "syscall"), ("a.rs", "h", "fn", ""): ("logic", "keep", "")}
    assert merge_unsafe(current, recorded) == [("a.rs", "f", "block", "boundary", "x", "keep", "syscall"),
                                              ("a.rs", "g", "block", "logic", "y", "unknown", "")]
    report = unsafe_report(current, recorded)
    assert len(report) == 2 and report[0].startswith("new unsafe site: a.rs g") and report[1].startswith("unsafe site gone: a.rs h"), report
    assert unsafe_report(current, {k: ("logic", "maybe", "") for k in current})[0].endswith("is not one of boundary/core-allocator/ownership/logic/test and keep/safe-api-possible/defect-risk/unknown")
    files, inventory, _, _ = scan()
    assert files > 100 and inventory, "the scan found nothing; the paths or patterns are wrong"
    assert len(unsafe_inventory()) > 300, "the unsafe inventory found too little; the patterns are wrong"
    print("source checks self-test: ok")


def main(argv):
    if "--self-test" in argv:
        return self_test()
    files, inventory, atomics, details = scan()
    sites = unsafe_inventory()
    if "--update" in argv:
        write(INVENTORY, "# Every unsafe impl Send / Sync (tools/lint/source.py --update). Review each change.\n", inventory)
        merged = merge_unsafe(sites, read_unsafe_tsv())
        write(UNSAFE_SITES, UNSAFE_HEADER, merged)
        old = {r[1]: int(r[0]) for r in read_tsv(ATOMICS)}
        if os.path.exists(ATOMICS) and "--allow-increase" not in argv:
            lowered = {f: min(n, old[f]) for f, n in atomics.items() if f in old}
        else:
            lowered = atomics
        write(ATOMICS, "# Weakened atomic orderings without a comment, per file (tools/lint/source.py). Counts only go down.\n",
              sorted((n, f) for f, n in lowered.items()))
        unknown = sum(1 for r in merged if r[5] == "unknown")
        print(f"source checks: {len(inventory)} Send/Sync impls, {sum(lowered.values())} uncommented orderings, "
              f"{len(merged)} unsafe sites recorded" + (f" ({unknown} with verdict `unknown`: classify them)" if unknown else ""))
        return

    failed = False
    for rel, line_no, line in REF_FREES:
        print(f"{rel}:{line_no}: arena free of a reference parameter (a protected borrow for the whole call; take the "
              f"block as a P<T>, an address or a raw slice, see tsrs_core::free_raw):\n  {line}")
        failed = True
    recorded = set(read_tsv(INVENTORY))
    current = set(inventory)
    for row in sorted(current - recorded):
        print(f"new unsafe impl {row[1]} for {row[2]} in {row[0]}: justify it with a SAFETY comment, then record it with "
              f"tools/lint/source.py --update")
        failed = True
    for row in sorted(recorded - current):
        print(f"unsafe impl {row[1]} for {row[2]} in {row[0]} is gone: record it with tools/lint/source.py --update")
        failed = True
    for line in unsafe_report(sites, read_unsafe_tsv()):
        print(line)
        failed = True
    baseline = {r[1]: int(r[0]) for r in read_tsv(ATOMICS)}
    for f, n in sorted(atomics.items()):
        if n > baseline.get(f, 0):
            print(f"{f}: {n} weakened atomic orderings without a comment (baseline {baseline.get(f, 0)}); say why the "
                  f"ordering is enough in a // comment on or just above the line:")
            for line_no, line in details[f]:
                print(f"  {f}:{line_no}  {line}")
            failed = True
    if failed:
        sys.exit(1)
    lower = sum(n - atomics.get(f, 0) for f, n in baseline.items() if atomics.get(f, 0) < n)
    print(f"source checks: ok ({files} files, {len(inventory)} Send/Sync impls, {sum(atomics.values())} uncommented "
          f"orderings, {len(sites)} unsafe sites, no arena free of a reference parameter)"
          + (f"; {lower} fewer than recorded, run --update" if lower else ""))


if __name__ == "__main__":
    main(sys.argv[1:])
