# <img src="assets/icon.svg" width="32" height="32" align="top" alt="" /> krop

A small Windows region screenshot tool written in Rust.

Windows Snipping Tool dumps everything into one flat folder, and ShareX has way more menus and settings than I ever use. `krop` is a single ~600 KB binary that sits in the system tray, uses around 5 MB of memory at 0% idle CPU, grabs `PrtSc`, and gets out of the way.

## How it works

Press `PrtSc` to freeze the screen across all monitors. Drag a box over what you want and let go of the mouse button. There is no annotation toolbar or confirmation step. Press `Esc` or right-click if you want to cancel.

As soon as you release the mouse:

1. The image goes straight to your clipboard so you can `Ctrl+V` right away.
2. Saves a PNG (`YYYY-MM-DD_HH-MM-SS.png`) to disk, or a JPG if you set that format in the tray.
3. A small pill badge pops up for a second to confirm the save without stealing window focus.

By default, screenshots go to `Pictures\Screenshots` sorted into monthly folders like `2026-10 October`. You can switch between `Daily`, `Monthly`, and `Yearly` folders, change the save location, choose `PNG` or `JPG` from the `Saved Format` submenu, or toggle "Start with Windows" from the tray icon. Settings are saved in `%APPDATA%\krop\config.toml`.

```toml
base_directory = 'C:\Users\Example\Pictures\Screenshots'
grouping_mode = 'Monthly'
saved_format = 'Jpg'
jpeg_quality = 90
```

`jpeg_quality` sets the JPG compression quality between `1` and `100`. Lower values make smaller files. It defaults to `90`.

## Windows Print Screen conflict

Windows 11 binds `PrtSc` to the built-in Snipping Tool by default. If that setting is on when `krop` starts, `krop` will offer to open Windows Keyboard Settings so you can turn it off. Once you toggle it off, `krop` claims the key automatically.

## Install

Download `krop.exe` from the [latest release](../../releases/latest) and run it. No installer required.

## Build

Requires Windows and a Rust toolchain.

```powershell
cargo build --release
```

The binary will be at `target\release\krop.exe`.
