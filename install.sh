#!/bin/sh
set -e

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
LATEST_URL="https://github.com/$REPO/releases/latest/download/$ASSET_NAME"

echo "Detected $OS $ARCH"
echo "Downloading $ASSET_NAME from $LATEST_URL..."

# Create temp directory
TMP_DIR=$(mktemp -d)
cleanup() {
    rm -rf "$TMP_DIR"
}
trap cleanup EXIT

# Download
if command -v curl >/dev/null 2>&1; then
    curl -fsSL "$LATEST_URL" -o "$TMP_DIR/skillz.tar.gz"
elif command -v wget >/dev/null 2>&1; then
    wget -qO "$TMP_DIR/skillz.tar.gz" "$LATEST_URL"
else
    echo "Error: curl or wget is required"
    exit 1
fi

# Extract
tar -xzf "$TMP_DIR/skillz.tar.gz" -C "$TMP_DIR"

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
