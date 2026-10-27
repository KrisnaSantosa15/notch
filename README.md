<p align="center">
  <img src="assets/notch-banner.jpg" alt="Notch Banner" width="100%" />
</p>

<h1 align="center">Notch</h1>

<p align="center">
  <img src="https://img.shields.io/badge/Author-Krisna%20Santosa-blue?style=flat-square" alt="Author Krisna Santosa" />
  <img src="https://img.shields.io/badge/License-AGPL%20v3-green?style=flat-square" alt="License: AGPL v3" />
  <img src="https://img.shields.io/badge/Platform-Windows%2010%20%7C%2011-0078D6?style=flat-square" alt="Platform" />
</p>

<p align="center">
  <b>MacBook-like dynamic notch at the top edge of Windows 10/11 screens.</b><br>
  Written in Rust without a web engine: exe ±1 MB, memory ±45 MB, and 0% CPU when idle.
</p>

## Running

## Installation

### WinGet (Windows Package Manager)

Install Notch directly via PowerShell or Windows Terminal:

```powershell
winget install KrisnaSantosa15.notch
```

### Manual / From Source

```powershell
git clone https://github.com/KrisnaSantosa15/notch.git
cd notch
cargo run --release
```

Produces `target\release\notch.exe`, ready to run directly.

## Configuration (`config.toml`)

Notch creates a configuration file at `%APPDATA%\Notch\config.toml` upon launch. You can easily open it by right-clicking the notch or tray icon and selecting **Edit config.toml**.

Supported settings:
- **Dimensions**: Closed/open width, height, and corner radius curvature.
- **Tabs**: Enable or disable specific tabs (`media`, `focus`, `shelf`, `tasks`).
- **Theme**: Accent color mode (`album_art` or `custom` hex code like `#3B82F6`).
- **Behavior**: Auto-close delay (ms) and hover-to-open toggle.

## Features

**Closed notch** dynamically updates content based on state: blinking eyes when idle, mini artwork and
equalizer when music is playing, currently sung lyric line, Focus countdown, task reminders,
privacy indicators (green dot for camera, orange dot for microphone),
and brief HUD overlays (volume, Caps Lock, charger, keyboard layout).

**Open notch** (hover mouse over notch) with frosted glass styling and system stats (CPU & RAM):

| Tab | Content |
|---|---|
| Music | Artwork, controls, progress bar (click to seek), lyrics toggle |
| Focus | Pomodoro 5/15/25/45 minutes with estimated finish time |
| Shelf | Drag files into the notch, drag them out anytime. Files are not copied |
| Tasks | To-do list. End with a time (`Meeting 15:30`, `Call Dina at 9:00`) for reminders |

Synchronized lyrics fetched from [LRCLIB](https://lrclib.net) **only when lyrics are toggled on**.
Click any lyric line to jump to that part of the song.

## Controls

| Action | Result |
|---|---|
| Hover mouse over notch | Opens |
| Win + Alt + N | Toggle open / close |
| Scroll over notch | Adjusts volume (in Shelf/Tasks: scrolls list) |
| Drag files onto notch | Adds to Shelf |
| Right click / Tray icon | Menu: Toggle, Start with Windows, Quit |

The notch automatically hides during games, fullscreen video, or presentations.
Data is saved to `%APPDATA%\Notch\state.txt`.

## Structure

- `main.rs`: data types, two stacked windows (glass backdrop + content), Windows message routing
- `events.rs`: state and input: hover, click, drag, keystrokes, timers, lyrics, reminders
- `view.rs`: closed and open notch layout and rendering
- `ui.rs`: canvas, shapes, and icons
- `media.rs`: currently playing track and artwork (Windows SMTC)
- `lyrics.rs`: fetch and parse LRC lyrics
- `shelf.rs`: drag-in/drag-out files, thumbnails
- `tasks.rs`: tasks and reminder times
- `store.rs`: save and load persistent state
- `system.rs`: volume, Caps Lock, battery, keyboard layout, glass blur, autostart
- `config.rs`: load, validate, and manage user settings from `config.toml`
- `update.rs`: background GitHub Releases update checker and notification
- `text.rs`: text rendering using Segoe UI font

## Testing

```
cargo test             # unit tests
cargo test -- --ignored  # includes fetching lyrics from the internet
```

## Debug

- `NOTCH_OPEN=music|focus|shelf|tasks`: open the notch without hovering
- `NOTCH_DUMP=frame.png`: dump the last frame to a PNG file
- `NOTCH_FAKE_MEDIA="title|artist|duration|position"`: simulate a playing track

## License

This project is licensed under the [GNU Affero General Public License v3.0](LICENSE) (AGPLv3) &copy; 2026 Krisna Santosa.

<hr>

<p align="center">
  Notch is open source. Built with Love by <b>Krisna Santosa</b>.
</p>
<p align="center">
  <img src="assets/notch-logo.jpg" width="100" alt="Notch Logo" style="border-radius: 20px;" />
</p>