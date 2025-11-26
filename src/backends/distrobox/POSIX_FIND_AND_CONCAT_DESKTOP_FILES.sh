set -o pipefail >/dev/null 2>&1 # TODO: Join with the following (without piping), as it is part of POSIX 2022, but Debian 13 Trixie and Ubuntu 24.10 Oracular Oriole are the first releases with a `dash` version (>=0.5.12-7) supporting this flag. See also https://github.com/koalaman/shellcheck/issues/2555 and https://metadata.ftp-master.debian.org/changelogs/main/d/dash/stable_changelog
set -eu

base16FunctionDefinition="$(cat <<'EOF'
base16() {
  if [ "$#" -eq 0 ]; then
    cat
  else
    printf '%s' "$1"
  fi | od -vt x1 -A n | tr -d '[[:space:]]'
}
EOF
)"

eval "$base16FunctionDefinition"

dumpDesktopFiles() {
  if ! [ -d "$1" ]; then
    return
  fi

  find "$1" -name '*.desktop' -not -exec grep -q '^[[:space:]]*NoDisplay[[:space:]]*=[[:space:]]*true[[:space:]]*$' '{}' \; -exec sh -c "$(set +o);$base16FunctionDefinition;"'printf '\''"%s"="%s"\n'\'' "$(base16 "$1")" "$(base16 <"$1")"' - '{}' \;
}

printf 'home_dir="%s"\n' "$(base16 "$HOME")"

printf '[system]\n'
dumpDesktopFiles /usr/share/applications

printf '[user]\n'
dumpDesktopFiles "$HOME/.local/share/applications"