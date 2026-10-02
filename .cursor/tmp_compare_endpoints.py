import re
from pathlib import Path

TS_DIR = Path("/Users/jiangker/project/immich/server/src/controllers")
RUST_DIR = Path("/Users/jiangker/project/immich/rust-server/src/routes")

EXCLUDE_PATHS = {"/.well-known/immich", "/custom.css", "/favicon.ico"}

METHOD_MAP = {
    "Get": "GET",
    "Post": "POST",
    "Put": "PUT",
    "Patch": "PATCH",
    "Delete": "DELETE",
    "Head": "HEAD",
}


def normalize_path_segments(base: str, route: str) -> str:
    base = base.strip("/")
    route = route.strip("/")
    if base and route:
        full = f"{base}/{route}"
    elif base:
        full = base
    else:
        full = route
    parts = [p for p in full.split("/") if p]
    return "/" + "/".join(parts) if parts else "/"


def ts_full_path(controller_base: str, route_path: str) -> str:
    p = normalize_path_segments(controller_base, route_path)
    for ex in EXCLUDE_PATHS:
        if p == ex or p.startswith(ex + "/"):
            return p
    return "/api" + (p if p.startswith("/") else "/" + p)


def parse_ts():
    endpoints = []
    controller_re = re.compile(r"@Controller\(([^)]*)\)")
    method_re = re.compile(r"@(Get|Post|Put|Patch|Delete|Head)\(([^)]*)\)")

    for fp in sorted(TS_DIR.glob("*.controller.ts")):
        text = fp.read_text()
        cm = controller_re.search(text)
        if not cm:
            continue
        raw = cm.group(1).strip()
        if raw == "":
            ctrl_base = ""
        elif raw.startswith("'") or raw.startswith('"'):
            ctrl_base = raw.strip("'\"")
        elif "RouteKey.Asset" in raw:
            ctrl_base = "assets"
        elif "RouteKey.User" in raw:
            ctrl_base = "users"
        else:
            ctrl_base = raw.strip("'\"")

        lines = text.splitlines()
        pending_methods = []
        for line in lines:
            for m in method_re.finditer(line):
                meth = METHOD_MAP[m.group(1)]
                arg = m.group(2).strip()
                if arg in ("", "''", '""'):
                    route = ""
                else:
                    route = arg.strip("'\"")
                pending_methods.append((meth, route))
            stripped = line.strip()
            if pending_methods and re.match(
                r"^(async\s+)?[a-zA-Z_][a-zA-Z0-9_]*\s*\(", stripped
            ):
                for meth, route in pending_methods:
                    full = ts_full_path(ctrl_base, route)
                    params = re.findall(r":([a-zA-Z_][a-zA-Z0-9_]*)", full)
                    endpoints.append((meth, full, fp.name, tuple(params)))
                pending_methods = []

        if pending_methods:
            for meth, route in pending_methods:
                full = ts_full_path(ctrl_base, route)
                params = re.findall(r":([a-zA-Z_][a-zA-Z0-9_]*)", full)
                endpoints.append((meth, full, fp.name, tuple(params)))

    return endpoints


def parse_rust_routes_text(text: str, source: str):
    endpoints = []
    route_start = re.compile(r"\.route\s*\(\s*")
    method_chain = re.compile(
        r"(?:axum::routing::)?(get|post|put|patch|delete|head)\s*\(",
        re.I,
    )

    i = 0
    while i < len(text):
        m = route_start.search(text, i)
        if not m:
            break
        start_paren = text.find("(", m.start())
        pos = start_paren + 1
        while pos < len(text) and text[pos] in " \t\n\r":
            pos += 1
        if pos >= len(text) or text[pos] not in '"':
            i = m.end()
            continue
        quote = text[pos]
        j = pos + 1
        path = ""
        while j < len(text):
            if text[j] == "\\":
                j += 2
                continue
            if text[j] == quote:
                break
            path += text[j]
            j += 1
        depth = 0
        k = start_paren
        while k < len(text):
            c = text[k]
            if c == "(":
                depth += 1
            elif c == ")":
                depth -= 1
                if depth == 0:
                    break
            k += 1
        block = text[start_paren + 1 : k]
        methods = [mm.group(1).upper() for mm in method_chain.finditer(block)]
        for meth in methods:
            params = re.findall(r"\{([a-zA-Z_][a-zA-Z0-9_]*)\}", path)
            endpoints.append((meth, path, source, tuple(params)))
        i = k + 1
    return endpoints


def parse_rust():
    all_eps = []
    for fp in sorted(RUST_DIR.glob("*.rs")):
        if fp.name == "mod.rs":
            continue
        all_eps.extend(parse_rust_routes_text(fp.read_text(), fp.name))
    return all_eps


def path_pattern(path: str) -> str:
    p = re.sub(r":[a-zA-Z_][a-zA-Z0-9_]*", "{}", path)
    p = re.sub(r"\{[a-zA-Z_][a-zA-Z0-9_]*\}", "{}", p)
    p = re.sub(r"\{\*[^}]+\}", "{*}", p)
    return p


def main():
    ts = parse_ts()
    rust = parse_rust()

    ts_set = set((m, p) for m, p, _, _ in ts)
    rust_set = set((m, p) for m, p, _, _ in rust)

    def ts_patterns():
        d = {}
        for m, p, f, params in ts:
            key = (m, path_pattern(p))
            d.setdefault(key, []).append((p, params, f))
        return d

    def rust_patterns():
        d = {}
        for m, p, f, params in rust:
            key = (m, path_pattern(p))
            d.setdefault(key, []).append((p, params, f))
        return d

    ts_pat = ts_patterns()
    rust_pat = rust_patterns()

    ts_missing = []
    for m, p, f, params in sorted(ts, key=lambda x: (x[1], x[0])):
        if (m, path_pattern(p)) not in rust_pat:
            ts_missing.append((m, p))

    rust_extra = []
    for m, p, f, params in sorted(rust, key=lambda x: (x[1], x[0])):
        if not p.startswith("/api"):
            continue
        if (m, path_pattern(p)) not in ts_pat:
            rust_extra.append((m, p))

    mismatches = []
    for key in set(ts_pat) & set(rust_pat):
        ts_items = ts_pat[key]
        rust_items = rust_pat[key]
        ts_params = set(t[1] for t in ts_items)
        rust_params = set(t[1] for t in rust_items)
        if ts_params != rust_params:
            ts_p = ts_items[0][0]
            rust_p = rust_items[0][0]
            mismatches.append((key[0], ts_p, ts_params, rust_p, rust_params))

    print("TS unique:", len(ts_set), "Rust unique:", len(rust_set))
    print("\n=== 1. TS endpoints with no Rust route ===")
    seen = set()
    for m, p in ts_missing:
        if (m, p) in seen:
            continue
        seen.add((m, p))
        print(f"{m} {p}")

    print("\n=== 2. Rust /api routes with no TS endpoint ===")
    seen = set()
    for m, p in rust_extra:
        if (m, p) in seen:
            continue
        seen.add((m, p))
        print(f"{m} {p}")

    print("\n=== 3. Path parameter name mismatches ===")
    for meth, ts_p, ts_ps, rust_p, rust_ps in sorted(mismatches, key=lambda x: x[1]):
        print(f"{meth} {ts_p}  <->  {rust_p}  (TS {ts_ps} vs Rust {rust_ps})")


if __name__ == "__main__":
    main()
