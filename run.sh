#!/bin/sh
# Benchpress launcher: downloads the latest release binary for this machine into a
# temporary directory, verifies its SHA-256 checksum, runs it and deletes it on exit.
#
#   curl -fsSL https://raw.githubusercontent.com/matx64/benchpress/main/run.sh | sh                     open the menu
#   curl -fsSL https://raw.githubusercontent.com/matx64/benchpress/main/run.sh | sh -s -- run quick     any benchpress command
#   curl -fsSL https://raw.githubusercontent.com/matx64/benchpress/main/run.sh | sh -s -- install       install to ~/.local/bin instead
#
# Environment: BENCHPRESS_INSTALL_DIR (default ~/.local/bin), BENCHPRESS_VERSION (a release tag;
# the copy attached to each release pins that release, otherwise latest),
# BENCHPRESS_DOWNLOAD_URL (a mirror holding the release assets; https:// or file://).

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
    command -v "$1" >/dev/null 2>&1 || fail "this script needs '$1'"
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
            if [ "$arch" = x86_64 ] && [ "$(sysctl -n sysctl.proc_translated 2>/dev/null)" = 1 ]; then
                arch=aarch64
            fi
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
    curl --proto "$protocols" --tlsv1.2 -fsSL "$1" -o "$2" || fail "download failed: $1"
}

main() {
    need curl
    need tar
    need uname
    target=$(detect_target)
    version=${BENCHPRESS_VERSION:-latest}
    protocols='=https'
    if [ -n "${BENCHPRESS_DOWNLOAD_URL:-}" ]; then
        base=${BENCHPRESS_DOWNLOAD_URL%/}
        protocols='=https,file'
    elif [ "$version" = latest ]; then
        base="https://github.com/$REPO/releases/latest/download"
    else
        base="https://github.com/$REPO/releases/download/$version"
    fi
    asset="benchpress-$target.tar.gz"
    dir=${BENCHPRESS_INSTALL_DIR:-$HOME/.local/bin}
    mode=run
    if [ "${1:-}" = install ]; then
        mode=install
        shift
        [ "$#" -eq 0 ] || fail "install takes no other arguments; run benchpress after installing"
    fi

    tmp=$(mktemp -d 2>/dev/null || mktemp -d -t benchpress)
    trap 'rm -rf "$tmp"' EXIT
    trap 'exit 130' INT
    trap 'exit 143' TERM
    trap 'exit 129' HUP

    say "Downloading $asset"
    download "$base/$asset" "$tmp/$asset"
    download "$base/$asset.sha256" "$tmp/$asset.sha256"
    expected=$(cut -d ' ' -f 1 <"$tmp/$asset.sha256")
    actual=$(sha256_of "$tmp/$asset")
    [ -n "$expected" ] && [ "$expected" = "$actual" ] || fail "checksum mismatch for $asset (expected $expected, got $actual)"

    tar -xzf "$tmp/$asset" -C "$tmp"
    chmod 755 "$tmp/benchpress"

    if [ "$mode" = install ]; then
        mkdir -p "$dir"
        mv "$tmp/benchpress" "$dir/benchpress"
        say "Installed benchpress to $dir/benchpress"
        case ":$PATH:" in
            *":$dir:"*) say "Run it with: benchpress" ;;
            *)
                say "$dir is not on your PATH. Add it with:"
                say "  echo 'export PATH=\"$dir:\$PATH\"' >> ~/.profile"
                ;;
        esac
        return 0
    fi

    bin="$tmp/benchpress"
    if ! "$bin" --version >/dev/null 2>&1; then
        cache=${XDG_CACHE_HOME:-$HOME/.cache}/benchpress-launcher
        mkdir -p "$cache"
        run_dir=$(mktemp -d "$cache/run.XXXXXX") || fail "could not create a directory in $cache"
        trap 'rm -rf "$tmp" "$run_dir"; rmdir "$cache" 2>/dev/null' EXIT
        mv "$bin" "$run_dir/benchpress"
        bin="$run_dir/benchpress"
        "$bin" --version >/dev/null 2>&1 || fail "the downloaded binary does not run here. Set TMPDIR to a directory that allows executables."
    fi

    trap ':' INT
    status=0
    if (: </dev/tty) 2>/dev/null; then
        BENCHPRESS_EPHEMERAL=1 "$bin" "$@" </dev/tty || status=$?
    else
        BENCHPRESS_EPHEMERAL=1 "$bin" "$@" || status=$?
    fi
    return "$status"
}

main "$@"
