set dotenv-load

# Version follows the nearest git tag via `git describe`; tagging a release is
# all that's needed to bump the version baked into the hook (NH_VERSION) and
# the CLI. Falls back to "unknown" only when built outside a git checkout.
export VERSION := `git describe --tags --always --dirty 2>/dev/null || echo "unknown"`

# Firmware line to build for: QT=5 for firmware 4.x (Qt 5.2.1, KoboRoot.tgz),
# QT=6 for firmware 5.18+ (Qt 6.5.2, Kobo.tgz + .adds). Pass it as
# `QT=5 just build-package` or `just QT=5 build-package`.
export QT := env_var_or_default("QT", "6")
tc_dir := if QT == "5" { ".forgejo/nickeltc" } else { ".forgejo/nickeltc-qt6" }
tc_image := if QT == "5" { "strayrose/nickeltc" } else { "strayrose/nickeltc-qt6" }
# Each firmware line has its own C library, so the CLI is linked per toolchain
# into its own target dir; a shared one would silently reuse the wrong binary.
cross := if QT == "5" { "arm-nickel-linux-gnueabihf" } else { "arm-kobo-linux-gnueabihf" }
cli_bin := "cli/target/qt" + QT + "/arm-unknown-linux-gnueabihf/release/nickel-storygraph-cli"

[group('helpers')]
default:
  @{{ just_executable() }} --list --justfile {{ justfile() }}

# Prepare the toolchain docker image for QT (5: NickelTC, 6: Qt 6.5.2; + Rust + resvg)
[group('build')]
build-tc:
  docker build {{ tc_dir }} --build-arg UID=$(id --user) --build-arg GID=$(id --group) --tag {{ tc_image }}

# Render modified svgs to png
[group('build')]
[working-directory('hook/res')]
build-res:
  #!/usr/bin/env sh
  for svg in *.svg; do
    png="${svg%.svg}.png"
    if [ ! -e "$png" -o "$svg" -nt "$png" ]; then
      echo "$png"
      resvg "$svg" "$png"
    fi
  done

# Build the nickel QT plugin (Qt5 or Qt6, per QT)
[group('build')]
[working-directory('hook')]
build-hook:
  make NH_QT_MAJOR={{ QT }}

# Build the rust CLI. cli/.cargo/config.toml names NickelTC's compiler (QT=5,
# as upstream); the Qt6 toolchain's prefix is set through the environment, which
# takes precedence over that file.
[group('build')]
[working-directory('cli')]
build-cli:
  CARGO_TARGET_ARM_UNKNOWN_LINUX_GNUEABIHF_LINKER={{ cross }}-gcc \
  CC_arm_unknown_linux_gnueabihf={{ cross }}-gcc \
  CXX_arm_unknown_linux_gnueabihf={{ cross }}-g++ \
  AR_arm_unknown_linux_gnueabihf={{ cross }}-ar \
  cargo build --release --target=arm-unknown-linux-gnueabihf --target-dir=target/qt{{ QT }}

[group('build')]
build: build-res build-hook build-cli

# Package files into installable archives.
# QT=6: firmware 5.18+ extracts Kobo.tgz under /usr/local/Kobo; the CLI and
# config go under mnt/onboard/.adds and must be copied to the device separately.
# QT=5: firmware 4.x extracts KoboRoot.tgz at / - hook and .adds in one archive.
[group('package')]
package:
  #!/usr/bin/env sh
  set -e
  rm -rf mnt
  mkdir -p mnt/onboard/.adds/NickelStorygraph
  cp {{ cli_bin }}            mnt/onboard/.adds/NickelStorygraph/cli
  cp hook/res/config_example.ini mnt/onboard/.adds/NickelStorygraph/
  if [ "$QT" = "5" ]; then
    rm -rf KoboRoot
    mkdir -p KoboRoot/usr/local/Kobo/imageformats
    cp hook/libstorygraph.so KoboRoot/usr/local/Kobo/imageformats/
    cp -R mnt KoboRoot/
    (cd KoboRoot && tar --owner=root --group=root -vczf ../KoboRoot.tgz . | grep '[^/]$')
    rm -r KoboRoot
  else
    (cd hook && make kobo NH_QT_MAJOR=6)
  fi

# Build the package within the toolchain docker container
[group('package')]
build-package:
  docker run --volume="$PWD:$PWD" --workdir="$PWD" --volume="nickeltc-cargo-cache:/cargo/registry" --env QT={{ QT }} --rm {{ tc_image }} just build package

# Copy the package onto a kobo device
[group('package')]
copy-package: build-package
  #!/usr/bin/env sh
  if [ "$QT" = "5" ]; then
    cp KoboRoot.tgz /media/$(whoami)/KOBOeReader/.kobo/
  else
    cp -r mnt/onboard/.adds/NickelStorygraph /media/$(whoami)/KOBOeReader/.adds/
    cp hook/Kobo.tgz /media/$(whoami)/KOBOeReader/.kobo/
  fi
  sudo eject /media/$(whoami)/KOBOeReader/

# Format all source files
[group('helpers')]
format:
  cd cli && cargo fmt
  clang-format -i hook/src/**/*.cc hook/src/*.cc
  clang-format -i hook/src/**/*.h hook/src/*.h

[group('helpers')]
clean:
  cd hook && make clean
  cd cli && cargo clean
  rm -fv KoboRoot.tgz hook/Kobo.tgz
  rm -rf mnt
  for f in hook/res/*.png; do git ls-files --error-unmatch $f > /dev/null 2>&1 || rm -v $f; done

# Run all checks: symbol regression against the 5.18 firmware (QT=6 only; there
# is no 4.x reference yet) + artifact checks (after `just build package`)
[group('test')]
test:
  if [ "$QT" = "6" ]; then python3 test/check_symbols.py; fi
  sh test/check_artifacts.sh

# Static checks (host target): rustfmt + clippy (warnings as errors) + unit tests.
[group('test')]
[working-directory('cli')]
lint:
  cargo fmt --check
  cargo clippy -- -D warnings
  cargo test

# Run `logread` over ssh
[group('helpers')]
logs:
  #!/usr/bin/env expect
  spawn ssh $::env(KOBO_SERVER)
  expect "password: "
  send "$::env(KOBO_PASSWORD)\r"
  expect -ex {[root@kobo ~]#}
  send "logread -f\r"
  interact
