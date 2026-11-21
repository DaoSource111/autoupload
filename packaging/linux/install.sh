#!/bin/sh
# Installs AutoUpload for the current user: binaries in ~/.local/bin, menu entry and icon.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
bin="${XDG_BIN_HOME:-$HOME/.local/bin}"
data="${XDG_DATA_HOME:-$HOME/.local/share}"
mkdir -p "$bin" "$data/applications" "$data/icons/hicolor/512x512/apps"
install -m 755 "$here/autoupload" "$here/autoupload-gui" "$bin/"
install -m 644 "$here/autoupload.png" "$data/icons/hicolor/512x512/apps/autoupload.png"
sed "s|^Exec=.*|Exec=$bin/autoupload-gui|" "$here/autoupload.desktop" > "$data/applications/autoupload.desktop"
echo "Installed in $bin. Open AutoUpload from your applications menu, or run: autoupload doctor"
case ":$PATH:" in *":$bin:"*) ;; *) echo "Note: $bin is not in your PATH." ;; esac
