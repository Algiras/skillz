#!/bin/sh
set -e

# Arguments: [version] [mode]
# version: "latest" or "vX.Y.Z" (default: latest)
# mode: "install" or "run" (default: install)
VERSION="${1:-latest}"
MODE="${2:-install}"

# Detect OS and Arch
OS="$(uname -s)"
ARCH="$(uname -m)"

case "$OS" in
    Linux)
        case "$ARCH" in
            x86_64)
                ASSET_NAME="skillz-linux-x86_64.tar.gz"
                ;;
            *)
                echo "Unsupported Linux architecture: $ARCH"
                exit 1
                ;;
        esac
        ;;
    Darwin)
        case "$ARCH" in
            x86_64)
                ASSET_NAME="skillz-macos-x86_64.tar.gz"
                ;;
            arm64)
                ASSET_NAME="skillz-macos-aarch64.tar.gz"
                ;;
            *)
                echo "Unsupported macOS architecture: $ARCH"
                exit 1
                ;;
        esac
        ;;
    *)
        echo "Unsupported OS: $OS"
        exit 1
        ;;
esac

# GitHub Repo
REPO="Algiras/skillz"

if [ "$VERSION" = "latest" ]; then
    DOWNLOAD_URL="https://github.com/$REPO/releases/latest/download/$ASSET_NAME"
else
    DOWNLOAD_URL="https://github.com/$REPO/releases/download/$VERSION/$ASSET_NAME"
fi

echo "Detected $OS $ARCH"
echo "Mode: $MODE"
echo "Version: $VERSION"
echo "Downloading from $DOWNLOAD_URL..."

# Create temp directory
TMP_DIR=$(mktemp -d)
cleanup() {
    # If mode is run, we don't cleanup immediately if we exec, but exec replaces shell so trap might not fire?
    # Actually exec replaces the process, so trap won't fire. We should cleanup before exec if possible, 
    # but we need the binary. 
    # For run mode, we'll extract to a temp location that the OS cleans up or we leave it.
    # Let's just not trap cleanup for run mode, or copy binary to /tmp/skillz-run-$PID
    if [ "$MODE" != "run" ]; then
        rm -rf "$TMP_DIR"
    fi
}
trap cleanup EXIT

# Download
if command -v curl >/dev/null 2>&1; then
    curl -fsSL "$DOWNLOAD_URL" -o "$TMP_DIR/skillz.tar.gz"
elif command -v wget >/dev/null 2>&1; then
    wget -qO "$TMP_DIR/skillz.tar.gz" "$DOWNLOAD_URL"
else
    echo "Error: curl or wget is required"
    exit 1
fi

# Extract
tar -xzf "$TMP_DIR/skillz.tar.gz" -C "$TMP_DIR"
chmod +x "$TMP_DIR/skillz"

if [ "$MODE" = "run" ]; then
    echo "🚀 Running Skillz..."
    # Shift args to pass remaining arguments to the binary
    # But sh -s arguments are positional. We used $1 and $2.
    # We can't easily pass "remaining" args via pipe to sh -s without complex parsing.
    # For now, just run it.
    exec "$TMP_DIR/skillz"
else
    # Install
    INSTALL_DIR="/usr/local/bin"
    if [ ! -w "$INSTALL_DIR" ]; then
        echo "Sudo is required to install to $INSTALL_DIR"
        if command -v sudo >/dev/null 2>&1; then
            sudo mv "$TMP_DIR/skillz" "$INSTALL_DIR/"
        else
            echo "Error: sudo not found. Cannot install to $INSTALL_DIR"
            exit 1
        fi
    else
        mv "$TMP_DIR/skillz" "$INSTALL_DIR/"
    fi

    echo "✅ Skillz installed successfully to $INSTALL_DIR/skillz"
    echo "Run 'skillz --help' to get started!"
fi
