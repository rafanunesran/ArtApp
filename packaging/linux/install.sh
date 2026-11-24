#!/bin/sh
# Instala o ArtApp para o usuário atual (~/.local/bin e o menu de aplicativos).
set -e
cd "$(dirname "$0")"
mkdir -p "$HOME/.local/bin" "$HOME/.local/share/applications"
install -m 755 artapp "$HOME/.local/bin/artapp"
sed "s|^Exec=.*|Exec=$HOME/.local/bin/artapp|" artapp.desktop > "$HOME/.local/share/applications/artapp.desktop"
echo "ArtApp instalado. Procure por ArtApp no menu ou rode: $HOME/.local/bin/artapp"
