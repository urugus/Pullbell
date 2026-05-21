# Changelog

All notable changes to Pullbell are documented in this file.

This project follows semantic versioning. Release tags use the `vMAJOR.MINOR.PATCH`
format and must match the version in `Cargo.toml`.

## Unreleased

- Added in-app update checks against the latest GitHub Release.
- Added menu actions for opening the release page and starting a Homebrew cask
  update when Pullbell is installed with Homebrew.

## 0.2.1

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
