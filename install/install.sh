#!/usr/bin/env bash
set -euo pipefail

VERSION="1.3.0"
INSTALL_DIR="${HOME}/.local/bin"
SHARE_DIR="${HOME}/.local/share/eltr"
PROJECT_DIR="$(cd "$(dirname "$0")/.." && pwd)"

print_usage() {
    echo "Usage: $0 [--uninstall] [--prefix <dir>]"
    echo "  --uninstall          Remove eltr installation"
    echo "  --prefix <dir>       Install to <dir> instead of ~/.local/bin"
    exit 0
}

UNINSTALL=0
while [[ $# -gt 0 ]]; do
    case "$1" in
        --uninstall) UNINSTALL=1; shift ;;
        --prefix) INSTALL_DIR="$2"; shift 2 ;;
        --help|-h) print_usage ;;
        *) echo "Unknown option: $1"; print_usage ;;
    esac
done

if [ "$UNINSTALL" -eq 1 ]; then
    echo "Uninstalling eltr..."
    rm -f "${INSTALL_DIR}/eltr"
    rm -rf "${SHARE_DIR}"
    echo "Done. eltr removed from ${INSTALL_DIR}"
    exit 0
fi

echo "=== Elitra Language Installer v${VERSION} (Linux) ==="

if ! command -v cargo &>/dev/null; then
    echo "Error: Rust/Cargo not found. Install it from https://rustup.rs"
    exit 1
fi

if [ -f "${INSTALL_DIR}/eltr" ]; then
    OLD_VER="$("${INSTALL_DIR}/eltr" --version 2>/dev/null || echo "unknown")"
    echo "Existing installation detected: ${OLD_VER}"
    echo "Upgrading to v${VERSION}..."
fi

mkdir -p "$INSTALL_DIR" "$SHARE_DIR/examples"

echo "Building Elitra v${VERSION} in release mode..."
cargo build --release --manifest-path "${PROJECT_DIR}/Cargo.toml"

echo "Installing binary to ${INSTALL_DIR}..."
cp "${PROJECT_DIR}/target/release/eltr" "${INSTALL_DIR}/eltr"
chmod +x "${INSTALL_DIR}/eltr"

echo "Installing examples to ${SHARE_DIR}/examples/..."
cp -r "${PROJECT_DIR}/examples/"* "${SHARE_DIR}/examples/"

if ! echo "$PATH" | tr ':' '\n' | grep -qx "${INSTALL_DIR}"; then
    echo ""
    echo "Warning: ${INSTALL_DIR} is not in your PATH."
    echo "Add this to your shell profile (~/.bashrc, ~/.zshrc, etc.):"
    echo "    export PATH=\"\$HOME/.local/bin:\$PATH\""
fi

echo ""
echo "Elitra Lang v${VERSION} installed successfully!"
echo "  eltr <file>      Run a script"
echo "  eltr             REPL mode"
echo "  eltr fmt         Format code"
echo "  eltr test        Run tests"
echo "  eltr lsp         Start LSP server"
echo "  eltr init        Create a new project"
echo "  eltr run         Run project from package.toml"
echo "  eltr install     Install a package"
echo "Examples: ${SHARE_DIR}/examples/"
