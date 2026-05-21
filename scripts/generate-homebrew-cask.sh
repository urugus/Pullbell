#!/usr/bin/env bash
set -euo pipefail

if [ "$#" -ne 4 ]; then
  echo "usage: $0 <version> <arm64-sha256> <x86_64-sha256> <output-file>" >&2
  exit 2
fi

version="$1"
arm64_sha256="$2"
x86_64_sha256="$3"
output_file="$4"

mkdir -p "$(dirname "$output_file")"

cat > "$output_file" <<CASK
cask "pullbell" do
  version "${version}"

  on_arm do
    sha256 "${arm64_sha256}"
    url "https://github.com/urugus/Pullbell/releases/download/v#{version}/pullbell-#{version}-aarch64-apple-darwin.zip"
  end

  on_intel do
    sha256 "${x86_64_sha256}"
    url "https://github.com/urugus/Pullbell/releases/download/v#{version}/pullbell-#{version}-x86_64-apple-darwin.zip"
  end

  name "Pullbell"
  desc "macOS menu bar app for GitHub pull request notifications"
  homepage "https://github.com/urugus/Pullbell"

  app "Pullbell.app"

  depends_on macos: ">= :monterey"
end
CASK
