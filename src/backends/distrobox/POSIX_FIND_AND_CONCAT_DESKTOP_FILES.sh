set -eu

base16FunctionDefinition="$(cat <<'EOF'
base16() (
  set -eu;
  if [ "$#" -eq 0 ]; then
    cat
  else
    printf '%s' "$1"
  fi | od -vt x1 -A n | tr -d '[[:space:]]'
)
EOF
)"

eval "$base16FunctionDefinition"

dumpDesktopFiles() {
  if ! [ -d "$1" ]; then
    return
  fi

  find "$1" -name '*.desktop' -not -exec grep -q '^[[:space:]]*NoDisplay[[:space:]]*=[[:space:]]*true[[:space:]]*$' '{}' \; -exec sh -c "$base16FunctionDefinition;"'printf '\''"%s"="%s"\n'\'' "$(base16 "$1")" "$(base16 <"$1")"' - '{}' \;
}

printf 'home_dir="%s"\n' "$(base16 "$HOME")"

printf '[system]\n'
dumpDesktopFiles /usr/share/applications

printf '[user]\n'
dumpDesktopFiles "$HOME/.local/share/applications"

set +eu