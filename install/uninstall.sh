#!/usr/bin/env bash
set -euo pipefail

INSTALL_DIR="${HOME}/.local/bin"
SHARE_DIR="${HOME}/.local/share/eltr"

print_usage() {
    echo "Usage: $0 [--prefix <dir>]"
    echo "  --prefix <dir>       Uninstall from <dir> instead of ~/.local/bin"
    exit 0
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --prefix) INSTALL_DIR="$2"; shift 2 ;;
        --help|-h) print_usage ;;
        *) echo "Unknown option: $1"; print_usage ;;
    esac
done

echo "Uninstalling eltr..."

if [ -f "${INSTALL_DIR}/eltr" ]; then
    rm -f "${INSTALL_DIR}/eltr"
    echo "  Removed ${INSTALL_DIR}/eltr"
else
    echo "  ${INSTALL_DIR}/eltr not found"
fi

if [ -d "${SHARE_DIR}" ]; then
    rm -rf "${SHARE_DIR}"
    echo "  Removed ${SHARE_DIR}"
else
    echo "  ${SHARE_DIR} not found"
fi

echo ""
echo "eltr has been uninstalled."
echo "To remove the source code, delete the project directory manually."
