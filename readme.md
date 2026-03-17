# 🎉 MediaChat 

Welcome to **MediaChat**, the app to send texts, images, videos, and audio directly to your friends' screens using Discord commands!

Inspired by the **CCB**, a collective of French streamers, MediaChat allows you to display images accompanied by text directly on your friends' screens. And for an even more fun experience, use it with [**Transparent Overlay**](https://github.com/ProbablyClem/transparent-overlay/releases) !

---

## 🚀 Features

- 🎤 **Send audio**
- 🎥 **Send video**
- 🎥 **Send images**
- 💬 **Send text** 
- 🖼️ **See who's up**

---

## ⚡ Installation

1. **clone it** :
    ```bash
    git clone https://github.com/R0bas/MediaChat
    ```
2. **go into folder** :
    ```bash
    cd MediaChat
    ```
3. ** Environnements variables** : 
    - Create a `.env` file in the root directory
    You will need to add the following variables:
    ```bash
        DISCORD_TOKEN=YOUR_DISCORD_TOKEN
        DISCORD_CLIENT_ID=YOUR_DISCORD_CLIENT_ID
        DISCORD_GUILD_ID=YOUR_DISCORD_GUILD_ID
        COBALT_URL=http://cobalt-api:9000/
        BACKEND_URL=http://localhost:3000
    ```
    - You can get your **DISCORD_TOKEN** by creating a bot on the [Discord Developer Portal](https://discord.com/developers/applications).
    - You can get your **DISCORD_CLIENT_ID** and **DISCORD_GUILD_ID** via the Discord App (right click on the Bot and on the Server to get the ID)
4. **use docker compose** :
    ```bash
    docker-compose up -d --build
    ```
---

## 🖥️ Native Overlay (Windows)

An alternative to Transparent Overlay — a lightweight Rust app that renders media directly on a fullscreen transparent window. No browser, no webview.

### Prerequisites

Install the following **once** on your Windows machine:

**1. Rust (MSVC toolchain)**
```powershell
winget install Rustlang.Rustup
# Then in a new terminal:
rustup default stable-x86_64-pc-windows-msvc
```

**2. Visual Studio Build Tools 2022** (C++ compiler)
```powershell
winget install Microsoft.VisualStudio.2022.BuildTools --override "--quiet --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended --norestart"
```

**3. LLVM** (required by some build dependencies)
```powershell
winget install LLVM.LLVM
```

**4. FFmpeg BtbN GPL Shared 7.1** (runtime + build)
```powershell
winget install BtbN.FFmpeg.GPL.Shared.7.1
```
Then add the FFmpeg `bin/` folder to your system PATH — find it under:
```
%LOCALAPPDATA%\Microsoft\WinGet\Packages\BtbN.FFmpeg.GPL.Shared.7.1_*\ffmpeg-*\bin
```
> `ffmpeg`, `ffprobe` and `ffplay` must all be reachable from PATH at runtime.

---

### Build

```bash
cd infrastructure/front/native
cargo build --release
```

The binary is output to `target/release/mediachat-native.exe`.

---

### Run

```bash
./target/release/mediachat-native.exe \
  --server <BACKEND_URL> \
  --room <your-discord-username>
```

| Argument | Description | Example |
|---|---|---|
| `--server` | Socket.IO backend URL (not the frontend) | `http://q0g4s...sslip.io` |
| `--room` | Your Discord username (the room to join) | `elkofy` |

> ⚠️ The `--server` URL is the **backend** URL, not the frontend. If you're self-hosting with Docker, expose the backend or use its internal subdomain.

---

### Flags

| Flag | Default |
|---|---|
| `--room` | `default` |
| `--server` | `http://localhost:3000` |

You can also set the server via environment variable:
```bash
export MEDIACHAT_SERVER=http://...
./mediachat-native.exe --room elkofy
```

---

## 🛠️ Technologies

- 🎨 **Frontend** : Vue + TailwindCSS.
- ⚙️ **Backend** : Node.js + Express.
- 🌐 **WebSocket** : Socket.IO
- 🐳 **Containerisation** : Docker.
- 🎮 **DiscordJS**: Generating Discord commands 

---

## 🤩 Join the Adventure

Want to contribute your magic touch? We love it! Here's how you can get involved :

1. **Fork the project**.
2. **Create a funky branch**:
    ```bash
    git checkout -b feature/awesome-idea
    ```
3. **Add your personal touch**:
    ```bash
    git commit -m "Added an amazing feature"
    ```
4. **Share your masterpiece**:
    ```bash
    git push origin feature/awesome-idea
    ```
5. **Submit a Pull Request** and become a MediaChat legend!

---

## 📜 Licence

This project is licensed under the [MIT License](LICENSE).

---
