#!/bin/sh
set -eu

version=${LLMFUCK_VERSION:-__LLMFUCK_VERSION__}
repository=MintCider/llmfuck
install_dir=${LLMFUCK_INSTALL_DIR:-"$HOME/.local/bin"}

if [ "$version" = "__LLMFUCK""_VERSION__" ]; then
  echo "This installer template must be downloaded from a tagged release." >&2
  exit 1
fi
case "$version" in
  v[0-9]*.[0-9]*.[0-9]*) ;;
  *) echo "Invalid release version: $version" >&2; exit 1 ;;
esac
case "$version" in
  *[!A-Za-z0-9._+-]*) echo "Invalid release version: $version" >&2; exit 1 ;;
esac

case "$(uname -s)" in
  Linux) os=linux ;;
  Darwin) os=macos ;;
  *) echo "Unsupported operating system: $(uname -s)" >&2; exit 1 ;;
esac

case "$(uname -m)" in
  x86_64|amd64)
    if [ "$os" = linux ]; then
      target=x86_64-unknown-linux-gnu
    else
      target=x86_64-apple-darwin
    fi
    ;;
  arm64|aarch64)
    if [ "$os" = macos ]; then
      target=aarch64-apple-darwin
    else
      echo "Linux ARM64 release binaries are not available." >&2
      exit 1
    fi
    ;;
  *) echo "Unsupported architecture: $(uname -m)" >&2; exit 1 ;;
esac

archive="llmfuck-$version-$target.tar.gz"
base_url="https://github.com/$repository/releases/download/$version"
temp_dir=$(mktemp -d)
trap 'rm -rf -- "$temp_dir"' EXIT HUP INT TERM

curl -fL --retry 3 --proto '=https' --tlsv1.2 "$base_url/$archive" -o "$temp_dir/$archive"
curl -fL --retry 3 --proto '=https' --tlsv1.2 "$base_url/SHA256SUMS" -o "$temp_dir/SHA256SUMS"
checksum_line=$(grep " $archive$" "$temp_dir/SHA256SUMS") || {
  echo "No checksum found for $archive" >&2
  exit 1
}

if [ "$os" = macos ]; then
  (cd "$temp_dir" && printf '%s\n' "$checksum_line" | shasum -a 256 --check)
else
  (cd "$temp_dir" && printf '%s\n' "$checksum_line" | sha256sum --check)
fi

tar -xzf "$temp_dir/$archive" -C "$temp_dir"
mkdir -p "$install_dir"
install -m 755 "$temp_dir/llmfuck-$version-$target/fuck" "$install_dir/fuck"

echo "Installed fuck $version to $install_dir/fuck"
case ":${PATH-}:" in
  *":$install_dir:"*) ;;
  *) echo "Add $install_dir to PATH, then open a new shell." ;;
esac
echo "Run 'fuck config' to configure a provider and shell integration."
