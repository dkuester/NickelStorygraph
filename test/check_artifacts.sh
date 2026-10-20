#!/usr/bin/env sh
# Post-build artifact checks for NickelStorygraph, for the build selected by QT
# (5: firmware 4.x, 6: firmware 5.18+; default 6).
#
# Test 2 — libstorygraph.so: correct ABI (ARM32), soname, exported handlers,
#          and linked against the matching Qt major version only.
# Test 3 — packaging: QT=6: Kobo.tgz extracts the hook under imageformats/
#          (relative to /usr/local/Kobo), and the .adds payload carries the
#          CLI + config. QT=5: KoboRoot.tgz carries both at their absolute
#          paths (usr/local/Kobo/imageformats/, mnt/onboard/.adds/).
#
# Run after `just build package` (or `just build-package`), on Linux with the
# binutils toolchain (readelf/nm) on PATH.
set -eu

# Portable ELF readers: Linux CI has readelf; macOS has llvm-objdump (via
# xcrun) but no llvm-readelf. `file` exists on both and reports the machine.
dynsec_cmd() {
  if command -v readelf >/dev/null 2>&1; then readelf -d "$1";
  elif command -v greadelf >/dev/null 2>&1; then greadelf -d "$1";
  elif command -v llvm-objdump >/dev/null 2>&1; then llvm-objdump -p "$1";
  elif command -v xcrun >/dev/null 2>&1; then xcrun llvm-objdump -p "$1";
  else echo "FAIL: no ELF dynamic-section reader"; exit 1; fi
}

QT=${QT:-6}
SO=hook/libstorygraph.so
TGZ=hook/Kobo.tgz
ADDS=mnt/onboard/.adds/NickelStorygraph
case "$QT" in
  5) OTHER=6 ;;
  6) OTHER=5 ;;
  *) echo "FAIL: QT must be 5 or 6, got '$QT'"; exit 1 ;;
esac
echo "Checking the Qt$QT build"

echo "== Test 2: libstorygraph.so =="
[ -f "$SO" ] || { echo "FAIL: $SO missing"; exit 1; }

machine=$(file -b "$SO")
echo "  file: $machine"
printf '%s' "$machine" | grep -q 'ARM' \
  || { echo "FAIL: expected ARM ELF, got '$machine'"; exit 1; }

dynsec_cmd "$SO" | grep -q 'SONAME.*libstorygraph.so' \
  || { echo "FAIL: wrong or missing SONAME"; exit 1; }

for sym in _nh_ReadingController__setVolume \
           _nh_ReadingMenuView__constructor \
           _nh_MainNavView__constructor \
           _nh_SupportedLocales__supportedLocales; do
  nm -D --defined-only "$SO" | grep -q " $sym\$" \
    || { echo "FAIL: handler $sym not exported"; exit 1; }
done

dynsec_cmd "$SO" | grep -q "libQt${QT}Core" \
  || { echo "FAIL: not linked against libQt${QT}Core"; exit 1; }
if dynsec_cmd "$SO" | grep -q "libQt${OTHER}" || nm -D "$SO" | grep -q "@Qt_${OTHER}"; then
  echo "FAIL: binary references Qt${OTHER} (should be Qt${QT} only)"
  exit 1
fi

echo "  OK: .so checks passed"

echo "== Test 3: packaging =="
if [ "$QT" = "5" ]; then
  TGZ=KoboRoot.tgz
  [ -f "$TGZ" ] || { echo "FAIL: $TGZ missing"; exit 1; }
  for entry in ./usr/local/Kobo/imageformats/libstorygraph.so \
               ./mnt/onboard/.adds/NickelStorygraph/cli \
               ./mnt/onboard/.adds/NickelStorygraph/config_example.ini; do
    tar -tzf "$TGZ" | grep -qx "$entry" \
      || { echo "FAIL: $TGZ does not contain $entry"; exit 1; }
  done
else
  [ -f "$TGZ" ] || { echo "FAIL: $TGZ missing"; exit 1; }
  tar -tzf "$TGZ" | grep -qx 'imageformats/libstorygraph.so' \
    || { echo "FAIL: Kobo.tgz does not contain imageformats/libstorygraph.so"; exit 1; }
fi

[ -f "$ADDS/cli" ] || { echo "FAIL: $ADDS/cli missing"; exit 1; }
[ -f "$ADDS/config_example.ini" ] || { echo "FAIL: $ADDS/config_example.ini missing"; exit 1; }

# The CLI is linked per toolchain; report the newest glibc it needs so a CLI
# linked against the wrong sysroot (2.35 on the Qt6 image) stands out.
glibc=$(objdump -T "$ADDS/cli" 2>/dev/null | grep -o 'GLIBC_[0-9.]*' | sort -uV | tail -1 || true)
echo "  cli needs: ${glibc:-unknown}"

echo "  OK: packaging checks passed"
echo "ALL CHECKS PASSED"
