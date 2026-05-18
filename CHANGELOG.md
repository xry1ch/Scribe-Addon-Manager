# Changelog

## v1.0.0 - 2026-05-18

Initial GitHub 1.0 release of Scribe Addon Manager.

### Added

- ESOUI/MMOUI addon browsing with most downloaded, recent, category, and search views.
- Addon install, update, update all, and uninstall workflows.
- Required dependency handling with optional dependencies available for explicit selection.
- SavedVariables cleanup option for uninstall and installed addon details.
- Compressed AddOns backups and restore support.
- HTTP cache for ESOUI/MMOUI metadata and remote images.
- Manager metadata for reliable installed-addon matching and update detection.
- BBCode-rendered addon details, changelogs, links, images, and ESO color markup.
- Dark modern Tauri desktop UI for installed addons, search, details, backups, settings, and cache management.

### Safety

- ZIP extraction validates paths before install or restore and rejects traversal, absolute paths, Windows drive prefixes, symlinks, and unsupported entries.
- Remote install and update verify MD5 hashes when ESOUI/MMOUI provides one.
- Uninstall removes only the selected addon folder.
- SavedVariables deletion uses exact file candidates only and never wildcard deletion.
- External links and rendered BBCode URLs are limited to `http` and `https`.

### Known Limitations

- ESOUI/MMOUI API behavior is unofficial and undocumented.
- Update detection may be conservative for unmanaged addons.
- Optional dependencies are not auto-installed unless explicitly selected.
- Users should back up before large changes.
