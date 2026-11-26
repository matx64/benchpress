#!/bin/sh
# Benchpress installer: downloads the latest release binary for this machine,
# verifies its SHA-256 checksum and installs it without sudo.
#
#   curl -fsSL https://benchpress.dev/install.sh | sh
#   curl -fsSL https://benchpress.dev/install.sh | sh -s -- run
#
# Environment: BENCHPRESS_INSTALL_DIR (default ~/.local/bin), BENCHPRESS_VERSION (default latest).

set -eu

REPO="matx64/benchpress"

say() {
    printf '%s\n' "$*" >&2
}

fail() {
    say "error: $*"
    exit 1
}

need() {
    command -v "$1" >/dev/null 2>&1 || fail "this installer needs '$1'"
}

detect_target() {
    os=$(uname -s)
    arch=$(uname -m)
    case "$arch" in
        arm64 | aarch64) arch=aarch64 ;;
        x86_64 | amd64) arch=x86_64 ;;
        *) fail "unsupported CPU architecture '$arch'. Benchpress supports x86_64 and aarch64." ;;
    esac
    case "$os" in
        Linux) echo "$arch-unknown-linux-musl" ;;
        Darwin)
            [ "$arch" = aarch64 ] || fail "Intel Macs are not supported. Benchpress runs on Apple Silicon."
            echo "aarch64-apple-darwin"
            ;;
        *) fail "unsupported OS '$os'. Benchpress runs on Linux and macOS." ;;
    esac
}

sha256_of() {
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1" | cut -d ' ' -f 1
    elif command -v shasum >/dev/null 2>&1; then
        shasum -a 256 "$1" | cut -d ' ' -f 1
    else
        fail "need sha256sum or shasum to verify the download"
    fi
}

download() {
    curl --proto '=https' --tlsv1.2 -fsSL "$1" -o "$2" || fail "download failed: $1"
}

main() {
    need curl
    need tar
    need uname
    target=$(detect_target)
    version=${BENCHPRESS_VERSION:-latest}
    if [ "$version" = latest ]; then
        base="https://github.com/$REPO/releases/latest/download"
    else
        base="https://github.com/$REPO/releases/download/$version"
    fi
    asset="benchpress-$target.tar.gz"
    dir=${BENCHPRESS_INSTALL_DIR:-$HOME/.local/bin}

    tmp=$(mktemp -d 2>/dev/null || mktemp -d -t benchpress)
    trap 'rm -rf "$tmp"' EXIT INT TERM

    say "Downloading $asset"
    download "$base/$asset" "$tmp/$asset"
    download "$base/$asset.sha256" "$tmp/$asset.sha256"
    expected=$(cut -d ' ' -f 1 <"$tmp/$asset.sha256")
    actual=$(sha256_of "$tmp/$asset")
    [ -n "$expected" ] && [ "$expected" = "$actual" ] || fail "checksum mismatch for $asset (expected $expected, got $actual)"

    tar -xzf "$tmp/$asset" -C "$tmp"
    mkdir -p "$dir"
    mv "$tmp/benchpress" "$dir/benchpress"
    chmod 755 "$dir/benchpress"
    say "Installed benchpress to $dir/benchpress"

    case ":$PATH:" in
        *":$dir:"*) bin=benchpress ;;
        *)
            bin="$dir/benchpress"
            say ""
            say "$dir is not on your PATH. Add it with:"
            say "  echo 'export PATH=\"$dir:\$PATH\"' >> ~/.profile"
            ;;
    esac

    if [ "$#" -gt 0 ]; then
        if (: </dev/tty) 2>/dev/null; then
            exec "$bin" "$@" </dev/tty
        fi
        exec "$bin" "$@"
    fi
    say ""
    say "Run it with: benchpress run"
}

main "$@"
