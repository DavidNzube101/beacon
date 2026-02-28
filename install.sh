#!/usr/bin/env sh
set -e

REPO="DavidNzube101/beacon"
INSTALL_DIR="/usr/local/bin"

# Get binary name from the first argument, default to "beacon"
BINARY="${1:-beacon}"

get_latest_version() {
    curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" \
        | grep '"tag_name"' \
        | sed 's/.*"tag_name": "\(.*\)".*/\1/'
}

get_target() {
    OS=$(uname -s)
    ARCH=$(uname -m)

    case "$OS" in
        Linux)
            case "$ARCH" in
                x86_64)  echo "beacon-linux-x86_64" ;;
                aarch64) echo "beacon-linux-arm64" ;;
                *)       echo "Unsupported architecture: $ARCH" >&2 && exit 1 ;;
            esac
            ;;
        Darwin)
            case "$ARCH" in
                x86_64) echo "beacon-macos-x86_64" ;;
                arm64)  echo "beacon-macos-arm64" ;;
                *)      echo "Unsupported architecture: $ARCH" >&2 && exit 1 ;;
            esac
            ;;
        *)
            echo "Unsupported OS: $OS" >&2
            exit 1
            ;;
    esac
}

# Check if the binary already exists before doing any work
if [ -e "$INSTALL_DIR/$BINARY" ]; then
    echo "Error: A file named '$BINARY' already exists in $INSTALL_DIR."
    echo "To install Beacon with a different name, pass it as an argument:"
    echo "  curl -fsSL https://raw.githubusercontent.com/DavidNzube101/beacon/master/install.sh | sh -s -- your-custom-name"
    echo ""
    echo "If you want to update the existing installation, please remove the old file first."
    exit 1
fi

VERSION=$(get_latest_version)
TARGET=$(get_target)
URL="https://github.com/$REPO/releases/download/$VERSION/$TARGET"

echo "Installing Beacon $VERSION as '$BINARY'..."
echo "Downloading $TARGET..."

TMP_BIN="/tmp/beacon_$(date +%s)"
curl -fsSL "$URL" -o "$TMP_BIN"
chmod +x "$TMP_BIN"

if [ -w "$INSTALL_DIR" ]; then
    mv "$TMP_BIN" "$INSTALL_DIR/$BINARY"
else
    echo "Sudo privileges are required to install to $INSTALL_DIR"
    sudo mv "$TMP_BIN" "$INSTALL_DIR/$BINARY"
fi

echo "Successfully installed to $INSTALL_DIR/$BINARY"
echo "Run: $BINARY --help"
