# Pullbell

Pullbell is a Rust macOS menu bar app for pull request notifications.
It uses GitHub OAuth Device Flow instead of personal access tokens, stores the
OAuth token in macOS Keychain, and polls GitHub for PRs that need attention.

## What this implements

This project follows the same product direction as [Neat](https://neat.run/):
a menu bar first workflow for PR notifications, with local state and minimal
noise. The current implementation focuses on Phase 1-4 MVP scope:

- GitHub OAuth Device Flow sign-in, no PAT required.
- Org/private repository support through OAuth scopes.
- Menu bar list for review requests, your open PRs, and unread PR notifications.
- Desktop notifications for newly seen actionable PR items.
- Token storage in macOS Keychain.
- Unit tests for PR merging and ordering.
- macOS CI for formatting, linting, and tests.

## Technical choices

- Rust for the app and GitHub API client.
- `tray-icon` + `tao` for a native macOS menu bar process.
- `reqwest` + `tokio` for GitHub OAuth and REST API calls.
- `keyring` for macOS Keychain storage.
- `mac-notification-sys` for native desktop notifications.

Tauri was intentionally not used for the MVP because the first usable version
does not need a full WebView UI. A richer PR preview window can be added later
without changing the GitHub/OAuth core modules.

## GitHub OAuth setup

Create a GitHub OAuth App and enable Device Flow.

Recommended development setup:

1. Open GitHub OAuth App settings.
2. Create or reuse an OAuth App.
3. Enable Device Flow in the app settings.
4. Store the client ID locally:

```sh
mkdir -p ~/.config/pullbell
printf '%s' 'YOUR_GITHUB_OAUTH_CLIENT_ID' > ~/.config/pullbell/client_id
```

You can also launch from a shell with:

```sh
PULLBELL_CLIENT_ID=YOUR_GITHUB_OAUTH_CLIENT_ID cargo run
```

Requested scopes:

- `repo`: needed for private org repositories.
- `read:org`: needed to discover org/team review requests.
- `notifications`: needed to read GitHub notification threads.

GitHub documents the OAuth Device Flow, OAuth scopes, and notifications API in:

- [Authorizing OAuth apps](https://docs.github.com/en/apps/oauth-apps/building-oauth-apps/authorizing-oauth-apps)
- [Scopes for OAuth apps](https://docs.github.com/en/apps/oauth-apps/building-oauth-apps/scopes-for-oauth-apps)
- [REST API notifications](https://docs.github.com/en/rest/activity/notifications?apiVersion=2022-11-28)

## Run

```sh
cargo run
```

Click the `PR` menu bar item, then choose `Sign in with GitHub`. The app opens
GitHub's device login page and copies the user code to the clipboard.

## Test

```sh
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
```

## Release

Releases are created from version tags.

1. Update `Cargo.toml` and `CHANGELOG.md` for the new version.
2. Commit the release changes.
3. Create and push a tag that matches the Cargo version:

```sh
git tag -a v0.1.0 -m "Release v0.1.0"
git push origin v0.1.0
```

The release workflow verifies formatting, linting, tests, and the tag/Cargo
version match. It then publishes unsigned macOS archives for:

- `aarch64-apple-darwin` for Apple Silicon Macs.
- `x86_64-apple-darwin` for Intel Macs.

Each GitHub Release includes a `checksums.txt` file with SHA-256 hashes for the
archives. Signed `.app` bundles and auto-update support are still planned for a
later packaging phase.

## Implementation plan

Completed MVP:

- Phase 1: Reference Neat's public behavior: menu bar, focused notifications,
  PR-oriented workflow, local-first state.
- Phase 2: Select Rust with a small native tray stack instead of a full WebView.
- Phase 3: Split app into OAuth, GitHub API, state, storage, and menu modules.
- Phase 4: Implement the app, tests, CI, and README.

Next phases:

- Add an in-app preview window for PR/comment bodies.
- Add user-configurable polling interval and repo/org filters.
- Add support for marking notification threads as done/read.
- Package as a signed `.app` bundle with auto-update support.
