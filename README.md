# ⚡ Hyper — Fast Video Downloader for Linux & Windows

**Hyper v1.0.0** by **Ben Lampard** — a lightweight, frameless desktop app for downloading videos from **YouTube, X (Twitter), Instagram, TikTok** and **1,750+ other sites** powered by [yt-dlp](https://github.com/yt-dlp/yt-dlp).

Built with **Tauri 2 + SvelteKit 5 + Rust + Tailwind** for speed, small binaries, and a native feel.

![License](https://img.shields.io/badge/license-MIT-blue) ![Tauri](https://img.shields.io/badge/Tauri-2.x-24C8DB) ![SvelteKit](https://img.shields.io/badge/SvelteKit-2-FF3E00) ![Platform](https://img.shields.io/badge/platform-Linux%20%7C%20Windows-lightgrey)

---

## ✨ Features

- **One-paste download** — paste any video URL, fetch metadata, pick quality and go
- **1,750+ sites** via bundled `yt-dlp` (see `SUPPORTED_SITES.txt`) — YouTube, X, Instagram, TikTok, Facebook, Reddit, Vimeo, Twitch… most sites just work
- **Quality choice** — `1080p / 720p / 480p / 360p / Audio Only (MP3)` with live file-size estimates from stream metadata
- **Smart Quality** — `Off / Best Available / Max File Size`. Picks the best `format_id` that fits your limit (e.g. `20 MB`) using a `pixels × fps × codec-efficiency` score; falls back to a two-pass `libx264` transcode (`95% → 90%` limit) with adaptive downscale `720p/480p` when needed
- **Real progress** — `yt-dlp --newline --progress` streamed over `download-progress` events, button itself fills
- **No sidecars** — original URL embedded losslessly inside the MP4 (`comment` / `purl` via `ffmpeg -c copy -map 0 -movflags +faststart`) — no `.url.txt`
- **Tidy library** — `~/Downloads/HyperDownloads` with list / 3-column tiles, newest/oldest/largest sort, search, play / show in folder / copy link / re-download / delete per item, thumbnails cached in `~/.cache/hyper/thumbs` (`320w` cover-art + `1s` frame fallback)
- **Polished UI** — frameless transparent card, `dark / light / system` themes, 8 swatch presets + custom accent colour wheel (taskbar bolt icon follows your accent), tiles vs list toggle, animated window resize (`880×680` settings side-panel)
- **Bundled tooling** — `yt-dlp` shipped in `src-tauri/bin/`, `ffmpeg/ffprobe` auto-downloaded on first run to `~/Downloads/HyperDownloads/.ffmpeg/` or used from system `PATH`

---

## 📦 Install on Linux

### Option A — Prebuilt release (recommended)

Grab the latest `.deb`, `.rpm` or `.AppImage` from **[Releases](https://github.com/DaBearLive/Hyper/releases)**:

**Ubuntu / Debian / Mint**
```bash
sudo dpkg -i hyper_1.0.0_amd64.deb
sudo apt-get install -f   # fix missing deps if any
# run
hyper
# or with Wayland fix (KDE)
GDK_BACKEND=x11 WEBKIT_DISABLE_COMPOSITING_MODE=1 hyper
```

**Fedora / RHEL / openSUSE**
```bash
sudo rpm -i hyper-1.0.0-1.x86_64.rpm
# or
sudo dnf install ./hyper-1.0.0-1.x86_64.rpm
```

**Arch / Manjaro — AppImage**
```bash
chmod +x hyper_1.0.0_amd64.AppImage
./hyper_1.0.0_amd64.AppImage
# Wayland users (transparent frameless window):
GDK_BACKEND=x11 WEBKIT_DISABLE_COMPOSITING_MODE=1 ./hyper_1.0.0_amd64.AppImage
```

**Launcher icon** is installed to `~/.local/share/applications/hyper.desktop` and `hicolor`; accent-colour changes rewrite `~/.cache/hyper/icon.png`.

### Option B — Build from source

**1. System dependencies**

*Arch / Manjaro*
```bash
sudo pacman -S --needed webkit2gtk-4.1 base-devel curl wget file openssl \
  appmenu-gtk-module libappindicator-gtk3 librsvg nodejs npm rustup \
  ffmpeg
rustup default stable
```

*Ubuntu / Debian*
```bash
sudo apt update
sudo apt install libwebkit2gtk-4.1-dev build-essential curl wget file \
  libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev \
  nodejs npm
# Rust via rustup: https://rustup.rs
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
# ffmpeg
sudo apt install ffmpeg
```

*Fedora*
```bash
sudo dnf install webkit2gtk4.1-devel openssl-devel curl wget file \
  libappindicator-gtk3-devel librsvg2-devel nodejs npm \
  rpm-build ffmpeg
rustup default stable
```

**2. Clone & build**
```bash
git clone https://github.com/DaBearLive/Hyper.git
cd Hyper
npm install
npm run tauri build
# output: src-tauri/target/release/bundle/{deb,rpm,appimage}/ + binary at src-tauri/target/release/hyper
```

Run the binary directly:
```bash
./src-tauri/target/release/hyper
# KDE Wayland:
GDK_BACKEND=x11 WEBKIT_DISABLE_COMPOSITING_MODE=1 ./src-tauri/target/release/hyper
```

> **Important:** Always use `npm run tauri build`, not plain `cargo build --release`. The Tauri CLI runs `beforeBuildCommand: npm run build` to generate the SvelteKit `build/` (adapter-static). Skipping it leaves an empty frontend and you’ll see `Could not connect to localhost: Connection refused`.

### yt-dlp & ffmpeg

- `yt-dlp` is bundled in `src-tauri/bin/` (checked in). Update it with `yt-dlp -U` or replace the binary.
- On first run Hyper looks for `ffmpeg` in `~/Downloads/HyperDownloads/.ffmpeg/`, then bundled, then system `PATH`. If missing it auto-downloads (Linux: `johnvansickle.com/ffmpeg` static build). You can also `sudo pacman -S ffmpeg` / `sudo apt install ffmpeg`.

---

## 🪟 Install on Windows

1. Download `Hyper_1.0.0_x64_en-US.msi` or `Hyper.exe` from Releases
2. Install & launch — `ffmpeg`/`yt-dlp` handling is the same (bundled `yt-dlp.exe`)
3. Videos save to `C:\Users\<you>\Downloads\HyperDownloads\`

---

## 🚀 Quick Start

1. **Paste** a video link → **Fetch** (shows title, thumbnail, extractor)
2. **Pick quality** — `Standard` dropdown or `Smart Quality` (`Best Available` auto-picks, `Max File Size` respects your default e.g. `20 MB`)
3. **Save to** shows `~/Downloads/HyperDownloads` — **Change** to pick another folder
4. **Download** — button fills with progress, then view switches to **Library**
5. In Library: toggle **List** / **Tiles (3-col)**, search/sort, `⋮` → **Play / Show in Folder / Copy Link / Re-download / Delete**

URL is stored **inside** the MP4 — delete the file and its `~/.cache/hyper/thumbs/<stem>.jpg` + legacy `*.url.txt` (if any) are removed.

---

## ⚙️ Development

```bash
git clone https://github.com/DaBearLive/Hyper.git
cd Hyper
npm install

# dev with hot-reload (Vite on http://localhost:1420)
npm run tauri dev

# type check
npm run check

# preview built frontend
npm run build && npm run preview

# production bundle (deb/rpm/appimage + binary)
npm run tauri build
```

**Dev troubleshooting**

| Symptom | Fix |
|---|---|
| `Could not connect to localhost: Connection refused` | `build/` is empty — you ran `cargo build` without `npm run build`. Run `npm run tauri build`. |
| Frameless window is black / transparent broken (KDE Wayland) | Launch with `GDK_BACKEND=x11 WEBKIT_DISABLE_COMPOSITING_MODE=1` — required for WebKitGTK transparent windows. |
| `ffmpeg not found` on first run | Install system ffmpeg or let Hyper auto-download to `~/Downloads/HyperDownloads/.ffmpeg/`. |
| `linuxdeploy` fails on `npm run tauri build` | Expected on Arch — `deb`/`rpm` + binary still produced in `target/release/`. `AppImage` needs `linuxdeploy` optional. |

---

## 📁 Project Structure

```
Hyper/
├── src/                 # SvelteKit frontend ( +page.svelte = whole UI, ~1100 lines )
│   └── app.html         # splash
├── src-tauri/
│   ├── src/lib.rs       # all backend: yt-dlp strategy, fetch, download, list, thumbs, settings, accent icon
│   ├── Cargo.toml       # image crate for dynamic bolt icon
│   ├── tauri.conf.json  # window 480×680 (max 900×700), frameless transparent
│   ├── capabilities/default.json
│   └── bin/yt-dlp{,.exe}
├── SUPPORTED_SITES.txt  # 1,752 yt-dlp extractors
└── build/               # SvelteKit static output (adapter-static)
```

**Config files:**
- `~/.config/hyper/settings.json` — `{smartMode,smartMaxVal,smartUnit,savePath,accentColor,theme}`
- `~/Downloads/HyperDownloads/` — videos
- `~/.cache/hyper/thumbs/` — 320w JPEGs
- `~/.cache/hyper/icon.png` — dynamic taskbar icon
- `~/.cache/hyper/ytdlp_strategy.txt` — remembered `yt-dlp` workaround (`clients` / `cookies:brave` etc.)

---

## 📄 License

MIT — see `LICENSE`. yt-dlp and ffmpeg retain their own licenses.

## 🙏 Credits

Made by **Ben Lampard** — Designed for Linux & Windows. Thanks to `yt-dlp`, `ffmpeg`, `Tauri`, `SvelteKit` contributors.
