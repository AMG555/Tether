#!/bin/sh
# One command between "finished" and "public": put your GitHub name in every place that needs it.
#
#   sh scripts/rename-owner.sh your-github-username
#
# Replaces the USER placeholder in Cargo.toml, both installers, the README and CONTRIBUTING.
# Idempotent: running it twice with the same name changes nothing.
set -eu

[ $# -eq 1 ] || { echo "usage: sh scripts/rename-owner.sh <github-username>" >&2; exit 1; }
owner=$1
case "$owner" in
  ""|-*|*[!A-Za-z0-9-]*) echo "not a valid GitHub username: $owner" >&2; exit 1 ;;
esac

root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"

files=$(grep -rl 'USER/savepoint' --include='*.toml' --include='*.md' --include='*.sh' \
        --include='*.ps1' --include='*.yml' . 2>/dev/null | grep -v '^./target/' | grep -v 'rename-owner')

[ -n "$files" ] || { echo "nothing to change — already set?"; exit 0; }

# BSD sed and GNU sed disagree about -i, so rewrite via a temp file.
for f in $files; do
  sed "s|USER/savepoint|$owner/savepoint|g" "$f" > "$f.tmp" && mv "$f.tmp" "$f"
  echo "  $f"
done

echo ""
echo "Now:"
echo "  grep -rn 'USER/savepoint' . --include='*.md' --include='*.sh' --include='*.ps1'   # expect nothing"
echo "  cargo test && node --test gui/*.test.js && sh scripts/test-install.sh"
echo "  git commit -am 'chore: point at $owner/savepoint'"
echo "  git tag v0.1.0 && git push origin main --tags"
