# Privacy Policy

_Last updated: October 28, 2026_

Notch is a desktop app that runs entirely on your PC. It has no account, no analytics, no telemetry and no ads. The developer does not collect, store or receive any of your data.

## What leaves your PC

Notch makes exactly two kinds of network requests, both over HTTPS:

| Request | When | What is sent | To |
|---|---|---|---|
| Synced lyrics | Only while lyrics are turned on in the Music tab, once per track | The playing track's title, artist and length | [LRCLIB](https://lrclib.net) (`lrclib.net`), a free public lyrics database |
| Update check | Once, a few seconds after Notch starts | Nothing besides a standard request for the latest release | GitHub (`api.github.com`) |

As with any web request, these services can see your IP address. Their own privacy policies apply to what they do with it. Notch sends no identifiers, file names, tasks or other personal content.

Lyrics are off by default. With lyrics off, the update check is the only request Notch makes.

## What stays on your PC

Notch reads the following locally to show it in the notch. None of it is sent anywhere:

- **Media:** the title, artist, artwork and playback position of whatever Windows reports as playing (System Media Transport Controls).
- **Camera and microphone status:** whether an app is currently using them, read from Windows' privacy settings. Notch never opens or records from the camera or microphone.
- **System status:** CPU and memory usage, battery and charging state, volume, Caps Lock and keyboard layout.
- **Fullscreen detection:** the size of the window in front, so Notch can hide during games, video and presentations. It does not read window contents.
- **Clipboard:** only when you paste into the task field yourself.

Notch saves the following in `%APPDATA%\Notch\`:

- `state.txt`: your tasks, the file paths on your Shelf, and a few preferences. Shelf files are never copied or uploaded. Notch only remembers where they are.
- `config.toml`: your settings.

If you turn on **Start with Windows**, Notch adds itself to your user's startup programs. You can turn this off at any time.

## Deleting your data

Quit Notch and delete the `%APPDATA%\Notch` folder. Uninstalling Notch and deleting that folder removes everything it stored.

## Children

Notch does not collect personal information from anyone, including children under 13.

## Changes

If Notch ever starts sending different data, this policy will be updated before that release, and the change will be listed in the [changelog](CHANGELOG.md).

## Contact

Questions or concerns: open an issue at [github.com/KrisnaSantosa15/notch/issues](https://github.com/KrisnaSantosa15/notch/issues).
