#!/usr/bin/env python3
"""Symbol regression test for the NickelStorygraph hook.

Every libnickel symbol referenced by the hook (the `nh_hook` `.sym = "..."` and
`nh_dlsym` `.name = "..."` entries in `nickelstorygraph.cc`) must exist in the
target firmware's exported symbol table. If a future firmware renames or removes
a symbol, or a symbol in the source is mistyped, this test fails.

The reference (`libnickel-5.18.syms.gz`) is the set of defined dynamic symbols of
`libnickel.so.1.0.0` from a 5.18 firmware image (BuildID
bcfe1a2b7dd40f6c53bbb049d59e0e25119adc21). To update it for a new firmware:
extract the new `libnickel.so.1.0.0` and run

    llvm-nm -D --defined-only libnickel.so.1.0.0 | awk '{print $NF}' \
      | grep -E '^_Z' | sort -u | gzip > test/libnickel-5.18.syms.gz
"""

import gzip
import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
HOOK_SRC = REPO / "hook" / "src" / "nickelstorygraph.cc"
REF = Path(__file__).resolve().parent / "libnickel-5.18.syms.gz"

# `.sym = "..."` (nh_hook) and `.name = "..."` (nh_dlsym). The `\s*` after the
# field name deliberately excludes `.sym_new = "..."` (our own handler names).
SYM_RE = re.compile(r'\.(?:sym|name)\s*=\s*"([^"]+)"')


def qt6_source(src: str) -> str:
    """Returns the source as the Qt6 build sees it.

    The hook switches a few ABI-dependent symbols with `#if NSG_QT6` /
    `#if !NSG_QT6`, `#else` and `#endif` (see nickelstorygraph.cc). Only the
    Qt6 branches are checked against the 5.18 reference; other `#if` blocks
    are kept as-is.
    """
    out = []
    stack = []  # per open #if: None (not ours) or whether its branch is kept
    for line in src.splitlines():
        directive = line.strip()
        if directive.startswith("#if"):
            if directive in ("#if NSG_QT6", "#if !NSG_QT6"):
                stack.append(directive == "#if NSG_QT6")
            else:
                stack.append(None)
            continue
        if directive.startswith("#else") and stack:
            if stack[-1] is not None:
                stack[-1] = not stack[-1]
            continue
        if directive.startswith("#endif") and stack:
            stack.pop()
            continue
        if all(keep is not False for keep in stack):
            out.append(line)
    return "\n".join(out)


def main() -> int:
    src = qt6_source(HOOK_SRC.read_text())
    # All libnickel symbols referenced by the hook are C++ mangled names
    # (prefix `_Z`). The `_Z` filter excludes non-symbol `.name` values such as
    # the plugin name in the `nh_plugin` registration struct.
    symbols = [s for s in SYM_RE.findall(src) if s.startswith("_Z")]
    if not symbols:
        print(f"ERROR: no symbols extracted from {HOOK_SRC}")
        return 1

    with gzip.open(REF, "rt") as f:
        reference = set(line.strip() for line in f)

    missing = sorted(s for s in symbols if s not in reference)
    print(f"extracted {len(symbols)} symbols; reference holds {len(reference)} symbols")
    if missing:
        print(f"ERROR: {len(missing)} symbol(s) not found in the 5.18 firmware:")
        for s in missing:
            print("  " + s)
        return 1
    print("OK: all symbols present in the 5.18 firmware")
    return 0


if __name__ == "__main__":
    sys.exit(main())
