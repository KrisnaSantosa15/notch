# Changelog

All notable changes to Notch are listed here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and versions follow [Semantic Versioning](https://semver.org/).

## [0.5.1] - 2026-10-28

### Added
- Typing a task that ends with a time now shows a bell, tints the time in the accent color, and adds a return button you can click instead of pressing Enter.
- Times like `9pm`, `9:30am` and `12:30pm` are recognized as reminders.
- Characters missing from Segoe UI (Chinese, Japanese, Korean, Thai, symbols, emoji) now render through system fallback fonts instead of empty boxes.
- Launch film and an animated demo in the README.

### Changed
- Shelf drag-in ripples now radiate from the notch outline instead of from inside it.
- Reminders ring an accent-colored bell and show their time on the right.
- Dotted times (`15.30`) only count as reminders after `at`, `jam`, `pukul` or `@`, so text like "Version 2.10" stays plain.

### Fixed
- `state.txt` and `config.toml` are written atomically, so a crash or power loss can no longer truncate tasks, Shelf items or settings.
- An unreadable `config.toml` is kept as `config.toml.bad` instead of being overwritten by defaults on the next settings change.
- Out-of-range sizes in `config.toml` are clamped instead of breaking rendering.
- A non-ASCII accent color no longer crashes the app.
- The notch now auto-closes after being opened with `Win + Alt + N` or from the tray.
- Turning off sound effects also silences timer and reminder alerts.
- Lyrics from the previous track can no longer appear on the next one.
- Lyrics and update requests give up after 10 seconds instead of hanging.
- Pre-releases such as `0.6.0-rc1` are no longer offered as updates over `0.6.0`.
- A fullscreen app on another monitor no longer hides the notch.

### Performance
- Camera and microphone status is checked every 3 seconds instead of every second.
- CPU and RAM usage are only sampled while the notch is open.
- The drawing canvas is reused between frames instead of being reallocated.

## [0.5.0] - 2026-10-28

### Added
- First-run welcome: an animated pill with eyes and short guide tips.
- In-notch Settings tab for tabs, dimensions, accent color, sound, hover-to-open, fullscreen hiding and autostart, with a one-click reset to defaults.
- Hover micro-player with playback controls in the closed notch.
- Split "satellite" bubble when music and the Focus timer run at the same time.
- Scroll over the notch to change volume, and scrub media.
- Subtle sound effects for interactions.
- Glowing progress line along the bottom rim for the timer and media.
- Cursor-following highlight on the glass rim.
- Pulsing underglow while music plays.
- Shelf expands when you drag a file toward it, with pulsing ripples.

### Changed
- The accent color now applies to the Focus ring, countdown and alert.
- Snappier spring animations.
- Official Notch logo for the tray icon.

### Fixed
- The rectangular blur halo around the open notch's corners is gone.
- The add-task bar now sits clear of the bottom curve.
- Missing-glyph boxes in the welcome text.
- The volume HUD shows in the header when scrolling while the notch is open.

## [0.4.1] - 2026-10-28

### Changed
- The volume HUD now uses Core Audio notifications instead of a low-level keyboard hook.

## [0.4.0] - 2026-10-27

### Added
- Global shortcut `Win + Alt + N` to open and close the notch.
- System tray icon with a menu and click-to-toggle.
- Privacy indicator: a dot when the webcam or microphone is in use.
- CPU and RAM usage in the open notch header.
- Multi-monitor support.
- `config.toml` for notch dimensions, auto-close timing, enabled tabs and a custom accent color.
- Update checker that points to new GitHub releases.
- WinGet package: `winget install KrisnaSantosa15.notch`.
- AGPL-3.0 license.

### Fixed
- The notch was sometimes hidden when no fullscreen app was running.
- Privacy indicator state detection and dot visibility.

## [0.3.0] - 2026-10-27

First public release.

### Added
- Closed notch with live activities: blinking eyes when idle, artwork and equalizer while music plays, the current lyric line, the Focus countdown, task reminders, and short HUDs for volume, Caps Lock, charging and keyboard layout.
- Open notch with frosted glass and four tabs:
  - **Music:** artwork, controls, and a seekable progress bar.
  - **Focus:** Pomodoro timer with 5, 15, 25 and 45 minute presets.
  - **Shelf:** drag files in and out. Files are never copied.
  - **Tasks:** to-do list where a trailing time becomes a reminder.
- Synced lyrics from LRCLIB, fetched only when lyrics are turned on. Click a line to jump to it.
- Auto-hide during games, fullscreen video and presentations.
- Start with Windows.

[0.5.1]: https://github.com/KrisnaSantosa15/notch/compare/v0.5.0...v0.5.1
[0.5.0]: https://github.com/KrisnaSantosa15/notch/compare/v0.4.1...v0.5.0
[0.4.1]: https://github.com/KrisnaSantosa15/notch/compare/v0.4.0...v0.4.1
[0.4.0]: https://github.com/KrisnaSantosa15/notch/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/KrisnaSantosa15/notch/releases/tag/v0.3.0
