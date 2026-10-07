#!/usr/bin/env python3
"""Reference implementation of the PROPOSED key derivation v1 (K5 final). Pure function of (path, body). No secret here:
`canonical(path, body)` returns the exact string to hash; `digest(...)` is unkeyed SHA-256 truncated to 16 bytes (hex),
the proxy would use HMAC-SHA-256 with a random secret over the same string."""
import hashlib, json, re, unicodedata
KEY_TEXT_LIMIT = 8192           # bytes per part (PROPOSED key_text_limit)
SEP = "\x1f"
BILLING_PREFIX = "x-anthropic-billing-header:"
# wrapper-only messages: skipped as "first part" and concatenated in front (Codex <environment_context>); PROPOSED list, configurable
WRAPPER_TAGS = ("<environment_context>", "<system-reminder>", "<user_instructions>", "<skills_instructions>")
TEXT_TYPES = ("text", "input_text", "output_text")
IGNORED_TYPES = ("thinking", "redacted_thinking", "reasoning", "tool_use", "tool_result", "function_call", "function_call_output")

def canon_text(t, limit=KEY_TEXT_LIMIT):
    t = unicodedata.normalize("NFC", t)
    t = re.sub(r"\s+", " ", t).strip()
    return t.encode("utf-8")[:limit].decode("utf-8", "ignore")

def content_text(c):
    if c is None: return ""
    if isinstance(c, str): return c
    out = []
    for p in c:
        if isinstance(p, str): out.append(p); continue
        ty = p.get("type")
        if ty in TEXT_TYPES: out.append(p.get("text", ""))
        elif ty in IGNORED_TYPES: continue
        elif ty: out.append(f"[{ty}]")
    return " ".join(out)

def is_wrapper_only(c):
    if isinstance(c, str): parts = [c]
    else: parts = [p.get("text", "") for p in (c or []) if isinstance(p, dict) and p.get("type") in TEXT_TYPES]
    parts = [p.lstrip() for p in parts if p.strip()]
    return bool(parts) and all(p.startswith(WRAPPER_TAGS) for p in parts)

def system_text(path, b):
    p = path.split("?")[0]
    pieces = []
    if p.endswith("/messages"):
        s = b.get("system")
        if isinstance(s, str): pieces.append(s)
        elif isinstance(s, list):
            pieces += [x.get("text", "") for x in s if x.get("type") == "text" and not x.get("text", "").startswith(BILLING_PREFIX)]
        msgs = b.get("messages", [])
    elif p.endswith("/responses"):
        if b.get("instructions"): pieces.append(b["instructions"])
        msgs = b.get("input", [])
        if isinstance(msgs, str): msgs = [{"role": "user", "content": msgs}]
    else:
        msgs = b.get("messages", [])
    # leading run of system/developer messages
    rest = []
    leading = True
    for m in msgs:
        if leading and m.get("role") in ("system", "developer"): pieces.append(content_text(m.get("content")))
        else: leading = False; rest.append(m)
    return " ".join(pieces), rest

def canonical(path, b):
    s, rest = system_text(path, b)
    texts = []; role = ""
    for m in rest:
        if m.get("role") in ("system", "developer"): continue      # later system messages are ignored (Claude Code)
        if m.get("type") in ("function_call", "function_call_output"): continue
        role = role or m.get("role", "")
        texts.append(content_text(m.get("content")))
        if not is_wrapper_only(m.get("content")): break
    return "v1" + SEP + "S" + canon_text(s) + SEP + "F" + role + SEP + canon_text(" ".join(texts))

def digest(path, b):
    return hashlib.sha256(canonical(path, b).encode()).hexdigest()[:32]
