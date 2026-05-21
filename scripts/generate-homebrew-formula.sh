#!/usr/bin/env bash
set -euo pipefail

if [ "$#" -ne 3 ]; then
  echo "usage: $0 <version> <source-sha256> <output-file>" >&2
  exit 2
fi

version="$1"
source_sha256="$2"
output_file="$3"

mkdir -p "$(dirname "$output_file")"

cat > "$output_file" <<FORMULA
class Pullbell < Formula
  desc "macOS menu bar app for GitHub pull request notifications"
  homepage "https://github.com/urugus/Pullbell"
  url "https://github.com/urugus/Pullbell/archive/refs/tags/v${version}.tar.gz"
  version "${version}"
  sha256 "${source_sha256}"
  license "MIT"
  head "https://github.com/urugus/Pullbell.git", branch: "main"

  depends_on "rust" => :build
  depends_on macos: :monterey

  def install
    system "cargo", "install", *std_cargo_args(root: libexec)
    system "scripts/package-macos-app.sh", libexec/"bin/pullbell", buildpath/"dist", version

    prefix.install "dist/Pullbell.app"

    (bin/"pullbell").write <<~SH
      #!/bin/bash
      exec /usr/bin/open "#{opt_prefix}/Pullbell.app"
    SH
    chmod 0755, bin/"pullbell"
  end

  def caveats
    <<~EOS
      Pullbell is installed as:
        #{opt_prefix}/Pullbell.app

      Start it with:
        pullbell

      Formula builds do not embed Pullbell's release OAuth client ID. If the app
      asks for one, create a GitHub OAuth App with Device Flow enabled and save
      its client ID:
        mkdir -p ~/.config/pullbell
        printf '%s' 'YOUR_GITHUB_OAUTH_CLIENT_ID' > ~/.config/pullbell/client_id
    EOS
  end

  test do
    assert_path_exists prefix/"Pullbell.app/Contents/MacOS/pullbell"
    assert_predicate prefix/"Pullbell.app/Contents/MacOS/pullbell", :executable?
  end
end
FORMULA
