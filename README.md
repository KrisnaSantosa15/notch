<p align="center">
  <img src="assets/notch-banner.jpg" alt="Notch Banner" width="100%" />
</p>

<h1 align="center">Notch</h1>

<p align="center">
  <b>MacBook-like dynamic notch at the top edge of Windows 10/11 screens.</b><br>
  Written in Rust without a web engine: exe ±1 MB, memory ±45 MB, and 0% CPU when idle.
</p>

## Running

```
cargo run --release
```

Produces `target\release\notch.exe`, ready to run directly.

## Features

**Closed notch** dynamically updates content based on state: blinking eyes when idle, mini artwork and
equalizer when music is playing, currently sung lyric line, Focus countdown, task reminders,
and brief HUD overlays (volume, Caps Lock, charger, keyboard layout).

**Open notch** (hover mouse over notch) with frosted glass styling:

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
| Scroll over notch | Adjusts volume (in Shelf/Tasks: scrolls list) |
| Drag files onto notch | Adds to Shelf |
| Right click | Menu: Start with Windows, Quit |

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

<hr>

<p align="center">
  Notch is open source. Built with Love by Krisna Santosa.
</p>
<p align="center">
  <img src="assets/notch-logo.jpg" width="100" alt="Notch Logo" style="border-radius: 20px;" />
</p>