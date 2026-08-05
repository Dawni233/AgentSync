# AgentSync

> Sync AI assistant configs across devices -- push your Claude Code / Cursor / Codex / ZCode settings to a private Git repo and switch between "personas" in one click.

**English** · [简体中文](./README.md)

---

## Why AgentSync

If you work across multiple machines (work laptop + personal laptop) and use several AI coding assistants, you've probably hit these pain points:

- Tweaked a prompt in `CLAUDE.md` on machine A, then had to manually copy it to machine B
- Wanted several config presets for the same agent ("deep coding" vs "code review") but could only swap files by hand
- Agent configs scattered across `~/.claude`, `~/.cursor`, `~/.codex`... with no unified entry point

AgentSync uses a private Git repo as the backend, auto-syncs these config files to the cloud, and provides a "Persona" mechanism to switch between config sets instantly.

## Key Features

- **Unified multi-agent management** -- 8 built-in presets (WorkBuddy / Claude Code / Cursor / Codex / ZCode / Qoder / OpenClaw / QwenPaw), or define custom agents with any config directory and sync rules
- **Private Git repo sync** -- Supports GitHub and Gitee with PAT auth. Config changes auto commit + push; other devices pull to sync.
- **Persona management** -- Save the current config as a named snapshot, switch anytime. Export as `.zip` persona packages, import with diff preview and risk warnings.
- **Two-level conflict resolution** -- L1 handles Git history conflicts (export patch / discard local), L2 handles config content conflicts (keep local / keep remote)
- **File preview** -- Side-by-side dual-pane comparison of "persona snapshot" vs "local current file", with render / source / diff views
- **Auto sync** -- Optional 5 / 15 / 30 / 60 min scheduled sync, with autostart and system tray support
- **Atomic writes** -- Two-level atomicity: file-level (write temp then rename) and agent-level (stage all files, overwrite only on full success), with automatic rollback on failure

## Preset Agents

| Agent | Config Dir | Synced Files |
|---|---|---|
| WorkBuddy | `~/.workbuddy` | SOUL.md, IDENTITY.md, USER.md, MEMORY.md, memory/** |
| Claude Code | `~/.claude` | CLAUDE.md, settings.json |
| Cursor | `~/.cursor` | rules/** |
| Codex | `~/.codex` | config.toml |
| ZCode | `~/.zcode` | AGENTS.md |
| Qoder | `~/.qoderworkcn` | commands/** |
| OpenClaw | `~/.openclaw` | identity/** |
| QwenPaw | `~/.qwenpaw` | HEARTBEAT.md, config.json, settings.json |

## Quick Start

### Download

Grab the installer for your platform from [Releases](https://github.com/Dawni233/AgentSync/releases):

- **Windows**: `AgentSync-v{version}-windows-x64-setup.exe` (NSIS installer)
- **macOS**: `AgentSync-v{version}-macos-{arch}.dmg`
- **Linux**: `AgentSync-v{version}-linux-amd64.deb`

### First Run

1. Create a **private** repo on GitHub or Gitee (empty is fine)
2. Generate a PAT (Personal Access Token) with repo read/write scope
3. Launch AgentSync and follow the onboarding wizard: Git platform, repo URL, username, PAT
4. Select the preset agents to sync (all enabled by default)
5. After setup, add or remove agents anytime from the Settings page

### Build from Source

**Prerequisites**:
- [Node.js](https://nodejs.org/) ≥ 18
- [Rust](https://www.rust-lang.org/) ≥ 1.77.2
- C++ build tools (Windows: Visual Studio Build Tools; macOS: Xcode Command Line Tools; Linux: `build-essential` etc.)

```bash
# Install frontend deps
npm install

# Dev mode (launches Tauri dev server)
npm run tauri dev

# Build release installers
npm run tauri build
```

Build artifacts land in `src-tauri/target/release/bundle/`.

## How It Works

### Sync Engine

The core is a state-machine-driven sync engine (`src-tauri/src/sync_engine.rs`):

```
S0 Create local snapshot (for rollback on failure)
  ↓
S1 git pull --rebase
  ↓
S2 Check local changes × remote updates
  ├─ Local changed, remote unchanged -> S4 write local to _current/ -> S6 commit+push
  ├─ Local unchanged, remote changed -> S5 write _current/ back to local
  ├─ Both changed -> L2 conflict (prompt user to choose)
  └─ Neither changed -> done
```

Each agent has a `_current/` directory in the Git repo representing its "current active config snapshot". During sync, the engine diffs the local config dir (e.g. `~/.claude`) against `_current/` to decide the sync direction.

### Data Storage

All data lives in the Tauri app data directory:

| Path | Purpose |
|---|---|
| `agentsync.db` | SQLite database (settings + agent cache tables) |
| `repo/` | Local clone of the Git repo |
| `repo/registry.json` | Agent registry |
| `repo/{agent_id}/_current/` | Agent's current active config snapshot |
| `repo/{agent_id}/{persona}/` | Persona snapshot directories |
| `snapshots/` | Pre-sync / pre-switch local snapshots (for rollback) |
| `tmp/` | Atomic write staging area |

App data directory location:
- Windows: `%APPDATA%\com.dawni.agentsync`
- macOS: `~/Library/Application Support/com.dawni.agentsync`
- Linux: `~/.local/share/com.dawni.agentsync`

## Tech Stack

**Frontend**: Vue 3 + TypeScript + Pinia + Vue Router + Vite 6

**Backend**: Rust + Tauri 2 + git2 (libgit2 bindings) + rusqlite + tokio

**Design**: Custom "Atelier Console" design system, offline fonts (Bricolage Grotesque / IBM Plex), no UI component library dependency.

## Project Structure

```
src/                      # Frontend
├── views/                # Three pages: Dashboard / Personalities / Settings
├── components/           # ConflictDialog / Onboarding / SyncStatusBadge
├── composables/          # useSync / usePersonalities / useToast
├── stores/               # Pinia stores (agents / settings / sync)
├── types/                # Shared frontend/backend types
└── utils/diff.ts         # Line-level diff (LCS algorithm)

src-tauri/src/            # Backend
├── lib.rs                # Tauri command registration + app entry + system tray
├── sync_engine.rs        # Sync state machine
├── git_sync.rs           # git2 wrappers (clone/pull/commit/push)
├── file_mapper.rs        # glob matching + bidirectional atomic copy + snapshot/rollback
├── persona.rs            # Persona management (save/switch/delete/import/export)
├── registry.rs           # registry.json + 8 preset agents
├── db.rs                 # SQLite storage
├── auto_sync.rs          # tokio scheduled sync
├── onboarding.rs         # First-run init flow
└── types.rs              # Shared type definitions
```

## Security

**Always use a private Git repo.** Agent config files may contain sensitive info (system prompts, personal preferences); a public repo would leak them.

Known security limitations in the current (MVP) release:

- **PAT stored in plaintext**: The Git PAT is currently stored as plaintext in the local SQLite database, not yet in the system keychain. Migration to keychain is planned.
- **CSP not enabled**: Tauri's CSP config is currently `null`; content security policy is not enforced.
- **Persona package import**: When importing `.zip` persona packages, zip entry paths are not validated against traversal (potential zip slip risk). Also, persona files are essentially AI system prompts -- a malicious persona package may contain prompt injection. Always review the diff preview before importing.
- **Path traversal protection**: The file preview interface validates `..` path components to prevent directory traversal.

## Development

```bash
# Type check
npm run typecheck

# Backend unit tests
cd src-tauri && cargo test
```

### Versioning

Follows Semantic Versioning (SemVer). The single source of truth for the version is the `version` field in `src-tauri/tauri.conf.json`, kept in sync with `package.json`.

## Contributing

Issues and Pull Requests are welcome. Before submitting a PR, please read [CONTRIBUTING.md](./CONTRIBUTING.md) and ensure:

1. `npm run typecheck` passes
2. `cargo test` passes
3. You follow the existing code style

## License

[MIT License](./LICENSE)
