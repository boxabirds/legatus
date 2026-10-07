#!/usr/bin/env python3
"""Heuristic checker for ASD-STE100-style writing in the specs/proxy documents.

It checks only what a script can check. It does not use the official ASD-STE100
dictionary, which was not available when this tool was written. Rules come from a
secondary summary of Issue 9, so treat a pass as "style rules met", not "compliant".

Run: uv run specs/proxy/tools/ste_lint.py specs/proxy/*.md
Exit code 1 means at least one ERROR. WARNINGs never change the exit code.
"""
import re
import sys
from pathlib import Path

MAX_WORDS_DESCRIPTIVE = 25
MAX_WORDS_PROCEDURE = 20
MAX_SENTENCES_PER_PARAGRAPH = 6
BANNED_WORDS = {"should", "could", "would", "might", "may", "shall"}
LATIN = re.compile(r"(?<![\w.])(e\.g\.|i\.e\.|etc\.|et al\.|vs\.)(?![\w])", re.I)
CONTRACTION = re.compile(r"\b\w+(n't|'re|'ve|'ll|'d|'m)\b", re.I)
IRREGULAR_PARTICIPLES = {"done", "made", "taken", "given", "seen", "written", "sent", "kept",
                         "held", "built", "chosen", "shown", "known", "found", "lost", "left", "set", "put", "read"}
BE = r"(?:is|are|was|were|be|been|being)"
PASSIVE = re.compile(rf"\b{BE}\s+(?:not\s+)?(\w+ed|{'|'.join(IRREGULAR_PARTICIPLES)})\b", re.I)
PERFECT = re.compile(r"\b(has|have|had)\s+(?:not\s+)?(\w+ed|been|\w+en|" + "|".join(IRREGULAR_PARTICIPLES) + r")\b", re.I)
PROGRESSIVE = re.compile(rf"\b{BE}\s+(\w+ing)\b", re.I)
IMPERATIVE_START = re.compile(r"^(?:\d+\.\s+|[-*]\s+)?(Set|Send|Start|Stop|Read|Write|Run|Open|Close|Check|Make|Use|Add|Remove|Record|Compare|Measure|Send|Find|Keep|Do not|Put|Take|Select|Start)\b")
ALLOW_ING = {"routing", "caching", "streaming", "logging", "queuing", "queueing", "scheduling", "affinity",
             "checkpointing", "batching", "processing", "sampling", "tokenizing", "prefilling", "decoding",
             "thinking", "reasoning", "warning", "string", "thing", "something", "nothing", "anything",
             "everything", "building", "testing", "monitoring", "binding", "pinning", "holding", "loading",
             "serving", "timing", "heading", "setting", "settings", "ceiling", "meaning", "ending", "morning",
             "during", "king", "bring", "spring", "ring", "wing", "sing", "swing", "being", "evening"}


def strip_noise(text: str) -> str:
    text = re.sub(r"```.*?```", "", text, flags=re.S)
    text = re.sub(r"`[^`]*`", "X", text)
    text = re.sub(r"\[([^\]]*)\]\([^)]*\)", r"\1", text)
    text = re.sub(r"<[^>]+>", "", text)
    return text


def blocks(text: str):
    """Yield (line_number, kind, text) for paragraphs and list items. Skips tables, headings, code."""
    in_code = False
    para, start = [], 0
    kind = "descriptive"
    heading_procedural = False

    def flush():
        nonlocal para
        if para:
            yield_item = (start, "procedure" if heading_procedural else kind, " ".join(para))
            para = []
            return yield_item
        return None

    for i, line in enumerate(text.splitlines(), 1):
        if line.strip().startswith("```"):
            in_code = not in_code
            item = flush()
            if item:
                yield item
            continue
        if in_code:
            continue
        s = line.strip()
        if s.startswith("#"):
            item = flush()
            if item:
                yield item
            heading_procedural = bool(re.search(r"procedure|steps|how to|run the|start the", s, re.I))
            continue
        if not s or s.startswith("|") or s.startswith(">") or s.startswith("---"):
            item = flush()
            if item:
                yield item
            continue
        if re.match(r"^(?:[-*]|\d+\.)\s+", s):
            item = flush()
            if item:
                yield item
            start, kind = i, "descriptive"
            para = [re.sub(r"^(?:[-*]|\d+\.)\s+", "", s)]
            item = flush()
            if item:
                yield item
            continue
        if not para:
            start = i
        para.append(s)
    item = flush()
    if item:
        yield item


def sentences(paragraph: str):
    parts = re.split(r"(?<=[.!?])\s+(?=[A-Z0-9`(\"])", paragraph.strip())
    return [p for p in parts if p]


def check(path: Path):
    errors, warnings = [], []
    raw = path.read_text(encoding="utf-8")
    if "—" in raw or "–" in raw:
        errors.append((0, "dash", "em or en dash found"))
    for line_no, kind, para in blocks(raw):
        clean = strip_noise(para)
        sents = sentences(clean)
        if len(sents) > MAX_SENTENCES_PER_PARAGRAPH and not re.match(r"^\s*(?:[-*]|\d+\.)", para):
            errors.append((line_no, "paragraph", f"{len(sents)} sentences (max {MAX_SENTENCES_PER_PARAGRAPH})"))
        for s in sents:
            words = re.findall(r"[A-Za-z0-9][\w'./-]*", s)
            limit = MAX_WORDS_PROCEDURE if (kind == "procedure" or IMPERATIVE_START.match(s)) else MAX_WORDS_DESCRIPTIVE
            if len(words) > limit:
                errors.append((line_no, "length", f"{len(words)} words (max {limit}): {s[:70]}..."))
            if ";" in s:
                errors.append((line_no, "semicolon", s[:70]))
            if LATIN.search(s):
                errors.append((line_no, "latin", LATIN.search(s).group(0)))
            if CONTRACTION.search(s):
                errors.append((line_no, "contraction", CONTRACTION.search(s).group(0)))
            for w in re.findall(r"[A-Za-z]+", s.lower()):
                if w in BANNED_WORDS:
                    errors.append((line_no, "word", f"'{w}' is not used: write 'must', 'can' or rewrite"))
            if PASSIVE.search(s):
                warnings.append((line_no, "passive", PASSIVE.search(s).group(0)))
            if PERFECT.search(s):
                warnings.append((line_no, "perfect", PERFECT.search(s).group(0)))
            if PROGRESSIVE.search(s):
                warnings.append((line_no, "progressive", PROGRESSIVE.search(s).group(0)))
            for w in re.findall(r"\b[a-z]{4,}ing\b", s.lower()):
                if w not in ALLOW_ING:
                    warnings.append((line_no, "-ing", w))
                    break
    return errors, warnings


def main(argv):
    files = [Path(a) for a in argv if a.endswith(".md")]
    total_err = 0
    for f in files:
        if "evidence" in f.parts:
            continue
        errors, warnings = check(f)
        total_err += len(errors)
        print(f"{f}: {len(errors)} errors, {len(warnings)} warnings")
        for ln, kind, msg in errors[:40]:
            print(f"  ERROR line {ln} [{kind}] {msg}")
        for ln, kind, msg in warnings[:15]:
            print(f"  warn  line {ln} [{kind}] {msg}")
    return 1 if total_err else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
