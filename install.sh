#!/bin/sh
# Installs tinyfleet's `fleet` binary.
#
#   curl -fsSL https://raw.githubusercontent.com/tinyish/fleet/main/install.sh | sh
#
# It never asks anything. Optional settings, as environment variables:
#   FLEET_INSTALL_DIR   where `fleet` goes (default: ~/.local/bin)
#   FLEET_VERSION       a release to pin, such as v0.3.0 (default: the latest)
#   FLEET_DOWNLOAD_URL  where the release files are (default: GitHub releases)
#   FLEET_TARGET        the platform to fetch (default: this machine's)
#
# Everything runs inside main, called on the last line, so a download cut
# short runs nothing.
set -eu

fail() {
    printf 'install.sh: %s\n' "$1" >&2
    exit 1
}

target() {
    os=$(uname -s)
    arch=$(uname -m)
    case "$arch" in
        arm64 | aarch64) arch=aarch64 ;;
        x86_64 | amd64) arch=x86_64 ;;
        *) fail "no fleet build for the CPU $arch" ;;
    esac
    case "$os" in
        Darwin) printf '%s-apple-darwin\n' "$arch" ;;
        Linux) printf '%s-unknown-linux-musl\n' "$arch" ;;
        *) fail "no fleet build for $os; tinyfleet runs on macOS and Linux" ;;
    esac
}

fetch() {
    if command -v curl >/dev/null 2>&1; then
        curl -fsSL "$1" -o "$2" || fail "could not download $1"
    elif command -v wget >/dev/null 2>&1; then
        wget -qO "$2" "$1" || fail "could not download $1"
    else
        fail "needs curl or wget to download fleet"
    fi
}

sha256() {
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1" | cut -d ' ' -f 1
    elif command -v shasum >/dev/null 2>&1; then
        shasum -a 256 "$1" | cut -d ' ' -f 1
    else
        fail "needs sha256sum or shasum to check the download"
    fi
}

main() {
    target=${FLEET_TARGET:-$(target)}
    case "$target" in
        aarch64-apple-darwin | x86_64-apple-darwin | aarch64-unknown-linux-musl | x86_64-unknown-linux-musl) ;;
        *) fail "no fleet build for $target" ;;
    esac

    if [ -n "${FLEET_VERSION:-}" ]; then
        case "$FLEET_VERSION" in
            v*) version=$FLEET_VERSION ;;
            *) version=v$FLEET_VERSION ;;
        esac
        base=${FLEET_DOWNLOAD_URL:-https://github.com/tinyish/fleet/releases/download/$version}
    else
        base=${FLEET_DOWNLOAD_URL:-https://github.com/tinyish/fleet/releases/latest/download}
    fi
    [ -n "${HOME:-}" ] || [ -n "${FLEET_INSTALL_DIR:-}" ] || fail "HOME is not set; set FLEET_INSTALL_DIR"
    dir=${FLEET_INSTALL_DIR:-$HOME/.local/bin}
    asset=fleet-$target.tar.gz

    work=$(mktemp -d)
    trap 'rm -rf "$work"; rm -f "$dir/.fleet.new"' EXIT
    trap 'exit 130' INT
    trap 'exit 143' TERM

    fetch "$base/$asset" "$work/$asset"
    fetch "$base/$asset.sha256" "$work/$asset.sha256"
    want=$(cut -d ' ' -f 1 <"$work/$asset.sha256")
    got=$(sha256 "$work/$asset")
    [ -n "$want" ] && [ "$want" = "$got" ] || fail "the download does not match its checksum; nothing was installed"

    tar -xzf "$work/$asset" -C "$work" fleet || fail "could not unpack $asset; nothing was installed"
    chmod 0755 "$work/fleet" || fail "could not make the download runnable; nothing was installed"
    installed=$("$work/fleet" --version) || fail "the downloaded fleet does not run on this machine; nothing was installed"

    mkdir -p "$dir" || fail "could not create $dir"
    cp "$work/fleet" "$dir/.fleet.new" || fail "could not write to $dir"
    chmod 0755 "$dir/.fleet.new" || fail "could not make $dir/.fleet.new runnable"
    mv -f "$dir/.fleet.new" "$dir/fleet" || fail "could not replace $dir/fleet"

    printf 'installed %s to %s\n' "$installed" "$dir"
    case ":${PATH:-}:" in
        *":$dir:"*) ;;
        *)
            printf '%s is not on your PATH; add this line to your shell profile:\n' "$dir"
            # The $PATH below is meant literally: it is the line to add.
            # shellcheck disable=SC2016
            printf '  export PATH="%s:$PATH"\n' "$dir"
            ;;
    esac
}

main "$@"
