# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

MediaChat is a Discord bot + web overlay app that lets users send media (images, videos, audio) and text directly to friends' screens via Discord slash commands. It is designed to work alongside [Transparent Overlay](https://github.com/ProbablyClem/transparent-overlay/releases).

## Commands

### Backend (root directory)
```bash
npm run build        # Compile TypeScript to dist/
npm run dev          # Build then run with nodemon (watch mode)
npm start            # Run compiled output from dist/
npm run lint         # Run ESLint on .ts files
```

### Frontend (infrastructure/front/vue/)
```bash
npm run dev          # Vite dev server
npm run build        # Type-check + build for production
npm run lint         # ESLint with auto-fix
npm run format       # Prettier format src/
```

### Full stack (Docker)
```bash
docker-compose up -d --build   # Build and start all services
```

## Required Environment Variables

Create a `.env` file in the root:
```
DISCORD_TOKEN=
DISCORD_CLIENT_ID=
DISCORD_GUILD_ID=
COBALT_URL=http://cobalt-api:9000/
BACKEND_URL=http://localhost:3000
```

The frontend uses `VITE_API_URL` (set in `infrastructure/front/vue/.env.production`) to connect to the backend socket.

## Architecture

The backend follows **Clean Architecture** with three layers:

- **`domain/`** — Core entities (`Mediachat`, `Author`, `Media`, `Image`, `Sound`, `Video`) and the `IMediaChatRepository` interface. No framework dependencies.
- **`application/usecases/`** — `CreateMediaChat` use case: constructs a `Mediachat` entity and delegates to the repository.
- **`infrastructure/`** — All framework-specific code:
  - `discord/` — Discord.js client setup; auto-loads all commands from `commands/*.js` at startup and registers them as guild slash commands via REST on `ClientReady`.
  - `express/` — Express server config, routes, and controllers. Routes: `POST /mediachats`, `GET /users`, `POST /cobalt`, `GET /tunnel`, `GET /token`.
  - `socket/SendMediaChat.ts` — Implements `IMediaChatRepository` by emitting Socket.IO events (`mediachat`) either to all clients or to a named room.
  - `front/vue/` — Vue 3 + Vite + TailwindCSS frontend. A single `MediaChatViewer` view receives Socket.IO events and displays a queue of media/text overlays. The viewer URL is `/viewer/:key` where `:key` is the Socket.IO room name.

**Data flow:** Discord command → `CreateMediaChat` use case → `SendMediaChat` (Socket.IO emit) → Vue frontend displays overlay.

**Cobalt integration:** The `sendurl` Discord command and `/cobalt` REST endpoint proxy requests to a self-hosted [Cobalt](https://github.com/imputnet/cobalt) instance for media URL extraction (YouTube, Instagram, etc.).

## Adding a New Discord Command

1. Create `infrastructure/discord/commands/<name>.ts` exporting `data` (a `SlashCommandBuilder`) and `execute` (async handler).
2. The command loader discovers `.js` files automatically after `npm run build`—no registration needed.
3. Commands are deployed to the guild on bot startup.

## Frontend Routing

The Vue app uses Vue Router. The viewer page is at `/viewer/:key`. The `:key` param is used as the Socket.IO room name—users join a room to receive targeted media. Use `/whosup` in Discord to see connected room names.
