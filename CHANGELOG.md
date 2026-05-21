# Changelog

All notable changes to Pullbell are documented in this file.

This project follows semantic versioning. Release tags use the `vMAJOR.MINOR.PATCH`
format and must match the version in `Cargo.toml`.

## Unreleased

- Nothing yet.

## 0.3.0

- Preserve the OAuth configuration error in the menu when sign-in cannot start.
- Fail release builds when the embedded GitHub OAuth client ID secret is missing.
- Refactored pull request notification tracking into a dedicated module.
- Added unit coverage for notification bootstrap, reset, and actionable item
  detection behavior.

## 0.2.1

- Added in-app update checks against the latest GitHub Release.
- Added menu actions for opening the release page and starting a Homebrew cask
  update when Pullbell is installed with Homebrew.
- Added ad-hoc signing for macOS release archives.
- Documented the local quarantine workaround for self-built app bundles.

## 0.2.0

- Added macOS `.app` bundle packaging for release artifacts.
- Added support for embedding a default GitHub OAuth client ID in release builds.
- Added release automation for updating the Homebrew Cask.

## 0.1.0

- Added the release workflow for tagged macOS builds.
- Added release documentation and a project license file.
- Initial MVP: GitHub OAuth Device Flow sign-in, menu bar PR lists, desktop
  notifications, Keychain token storage, and CI.
