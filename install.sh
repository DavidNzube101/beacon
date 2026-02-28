#!/usr/bin/env sh
set -e

REPO="DavidNzube101/beacon"
BINARY="beacon"
INSTALL_DIR="/usr/local/bin"

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
                *)       echo "oops! unsupported architecture: $ARCH" && exit 1 ;;
            esac
            ;;
        Darwin)
            case "$ARCH" in
                x86_64) echo "beacon-macos-x86_64" ;;
                arm64)  echo "beacon-macos-arm64" ;;
                *)      echo "oops! unsupported architecture: $ARCH" && exit 1 ;;
            esac
            ;;
        *)
            echo "Unsupported OS: $OS"
            exit 1
            ;;
    esac
}

VERSION=$(get_latest_version)
TARGET=$(get_target)
URL="https://github.com/$REPO/releases/download/$VERSION/$TARGET"

echo "Installing Beacon $VERSION..."
echo "Downloading $TARGET..."

curl -fsSL "$URL" -o "/tmp/$BINARY"
chmod +x "/tmp/$BINARY"

if [ -w "$INSTALL_DIR" ]; then
    mv "/tmp/$BINARY" "$INSTALL_DIR/$BINARY"
else
    sudo mv "/tmp/$BINARY" "$INSTALL_DIR/$BINARY"
fi

echo "Beacon installed to $INSTALL_DIR/$BINARY"
echo "Run: beacon --help"
