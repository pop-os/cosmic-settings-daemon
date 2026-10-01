name := 'cosmic-settings-daemon'
rootdir := ''
prefix := '/usr'
profile := 'release'
cargo-target-dir := env('CARGO_TARGET_DIR', 'target')

base-dir := absolute_path(clean(rootdir / prefix))
bin-dst := base-dir / 'bin' / name
system-actions-conf := base-dir / 'share/cosmic/com.system76.CosmicSettings.Shortcuts/v1/system_actions'
polkit-rule := base-dir / 'share/polkit-1/rules.d/cosmic-settings-daemon.rules'

# Compile with release profile by default
[private]
default: build-release

# Compile with debug profile
build-debug *args:
    cargo build --bin {{ name }} {{ args }}

# Compile with release profile
build-release *args: (build-debug '--release' args)

# Compile with a vendored tarball
build-vendored *args: vendor-extract (build-release '--frozen --offline' args)

# Remove Cargo build artifacts
clean:
    cargo clean

# Also remove .cargo and vendored dependencies
clean-dist: clean
    rm -rf .cargo vendor vendor.tar

# Install the binary and configuration files (use profile=debug for debug builds)
install:
    install -Dm0755 '{{ cargo-target-dir / profile / name }}' '{{ bin-dst }}'
    install -Dm0644 data/system_actions.ron '{{ system-actions-conf }}'
    install -Dm0644 data/polkit-1/rules.d/cosmic-settings-daemon.rules '{{ polkit-rule }}'

# Vendor Cargo dependencies locally
vendor:
    #!/usr/bin/env sh
    set -eu
    mkdir -p .cargo
    cargo vendor --locked > .cargo/config.toml
    tar pcf vendor.tar vendor
    rm -rf vendor

# Extract vendored dependencies
[private]
vendor-extract:
    rm -rf vendor
    tar pxf vendor.tar
