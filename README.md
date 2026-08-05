# AgentSync

> 跨设备的 AI 助手配置同步工具 —— 把你的 Claude Code / Cursor / Codex / ZCode 等配置统一同步到 Git 私有仓库，并在多套"人格"间快速切换。

[English](./README.en.md) · **简体中文**

---

## 为什么需要 AgentSync

如果你同时在多台设备上工作（公司电脑 + 个人电脑），又使用多个 AI 编程助手，你大概率遇到过这些问题：

- 在 A 机器调好了 `CLAUDE.md` 的提示词，到 B 机器还要手动复制一遍
- 想给同一个 Agent 准备几套不同场景的配置（"专注编码" vs "代码审查"），却只能靠手动改文件
- 各个 Agent 的配置散落在 `~/.claude`、`~/.cursor`、`~/.codex`…… 没有统一的管理入口

AgentSync 用一个 Git 私有仓库作为后端，把这些配置文件自动同步到云端，并提供"人格（Persona）"机制让你在多套配置间一键切换。

## 核心特性

- **多 Agent 统一管理** —— 内置 8 个预设（WorkBuddy / Claude Code / Cursor / Codex / ZCode / Qoder / OpenClaw / QwenPaw），也可自定义任意 Agent 的配置目录与同步规则
- **Git 私有仓库同步** —— 支持 GitHub 与 Gitee，用 PAT 认证，配置变更自动 commit + push，多设备 pull 即可同步
- **人格（Persona）管理** —— 把当前配置保存为命名快照，随时切换；支持导出为 `.zip` 人格包、从包导入（含 diff 预览与风险提示）
- **两级冲突处理** —— L1 处理 Git 历史冲突（可导出 patch / 放弃本地），L2 处理配置内容冲突（保留本地 / 保留远程）
- **文件预览** —— 左右双栏并排对比"人格快照"与"本地当前文件"，支持渲染 / 源码 / 差异三种视图
- **自动同步** —— 可选 5 / 15 / 30 / 60 分钟定时同步，支持开机自启动与系统托盘
- **原子写入** —— 文件级（先写临时文件再 rename）与 Agent 级（全部写暂存区成功才覆盖）两级原子性，同步失败自动回滚

## 预设 Agent

| Agent | 配置目录 | 同步内容 |
|---|---|---|
| WorkBuddy | `~/.workbuddy` | SOUL.md, IDENTITY.md, USER.md, MEMORY.md, memory/** |
| Claude Code | `~/.claude` | CLAUDE.md, settings.json |
| Cursor | `~/.cursor` | rules/** |
| Codex | `~/.codex` | config.toml |
| ZCode | `~/.zcode` | AGENTS.md |
| Qoder | `~/.qoderworkcn` | commands/** |
| OpenClaw | `~/.openclaw` | identity/** |
| QwenPaw | `~/.qwenpaw` | HEARTBEAT.md, config.json, settings.json |

## 快速开始

### 下载安装

前往 [Releases](https://github.com/Dawni233/AgentSync/releases) 下载对应平台的安装包：

- **Windows**: `AgentSync-v{版本号}-windows-x64-setup.exe`（NSIS 安装器）
- **macOS**: `AgentSync-v{版本号}-macos-{架构}.dmg`
- **Linux**: `AgentSync-v{版本号}-linux-amd64.deb`

### 首次使用

1. 在 GitHub 或 Gitee 创建一个**私有**仓库（空仓库即可）
2. 生成一个 PAT（Personal Access Token），需要仓库读写权限
3. 启动 AgentSync，按向导填写：Git 平台、仓库 URL、用户名、PAT
4. 勾选要同步的预设 Agent（默认全选）
5. 完成初始化后，在设置页可随时添加 / 删除 Agent

### 从源码构建

**环境要求**：
- [Node.js](https://nodejs.org/) ≥ 18
- [Rust](https://www.rust-lang.org/) ≥ 1.77.2
- 系统 C++ 构建工具（Windows 装 Visual Studio Build Tools；macOS 装 Xcode Command Line Tools；Linux 装 `build-essential` 等）

```bash
# 安装前端依赖
npm install

# 开发模式（启动 Tauri 开发服务器）
npm run tauri dev

# 构建发布安装包
npm run tauri build
```

构建产物位于 `src-tauri/target/release/bundle/`。

## 工作原理

### 同步引擎

AgentSync 的核心是一个状态机驱动的同步引擎（`src-tauri/src/sync_engine.rs`）：

```
S0 创建本地快照（用于失败回滚）
  ↓
S1 git pull --rebase
  ↓
S2 判断本地变更 × 远程新内容
  ├─ 本地有变更、远程无新内容 → S4 本地写入 _current/ → S6 commit+push
  ├─ 本地无变更、远程有新内容 → S5 _current/ 写回本地
  ├─ 两边都有变更 → L2 冲突（弹窗让用户选择）
  └─ 两边都无变更 → 结束
```

每个 Agent 在 Git 仓库里有一个 `_current/` 目录，代表"当前激活配置快照"。同步时，引擎比较本地配置目录（如 `~/.claude`）与 `_current/` 的差异，决定同步方向。

### 数据存储

所有数据存放在 Tauri 应用数据目录：

| 路径 | 用途 |
|---|---|
| `agentsync.db` | SQLite 数据库（设置表 + agent 缓存表） |
| `repo/` | Git 仓库本地克隆 |
| `repo/registry.json` | Agent 注册表 |
| `repo/{agent_id}/_current/` | Agent 当前激活配置快照 |
| `repo/{agent_id}/{persona}/` | 各人格快照目录 |
| `snapshots/` | 同步 / 切换前的本地快照（回滚用） |
| `tmp/` | 原子写入暂存区 |

应用数据目录路径：
- Windows: `%APPDATA%\com.dawni.agentsync`
- macOS: `~/Library/Application Support/com.dawni.agentsync`
- Linux: `~/.local/share/com.dawni.agentsync`

## 技术栈

**前端**：Vue 3 + TypeScript + Pinia + Vue Router + Vite 6

**后端**：Rust + Tauri 2 + git2（libgit2 绑定）+ rusqlite + tokio

**设计**：自研 "Atelier Console" 设计系统，离线字体（Bricolage Grotesque / IBM Plex），不依赖任何 UI 组件库。

## 项目结构

```
src/                      # 前端
├── views/                # 三个页面：Dashboard / Personalities / Settings
├── components/           # ConflictDialog / Onboarding / SyncStatusBadge
├── composables/          # useSync / usePersonalities / useToast
├── stores/               # Pinia stores（agents / settings / sync）
├── types/                # 前后端共享类型
└── utils/diff.ts         # 行级 diff（LCS 算法）

src-tauri/src/            # 后端
├── lib.rs                # Tauri 命令注册 + 应用入口 + 系统托盘
├── sync_engine.rs        # 同步状态机
├── git_sync.rs           # git2 封装（clone/pull/commit/push）
├── file_mapper.rs        # glob 匹配 + 双向原子拷贝 + 快照回滚
├── persona.rs            # 人格管理（保存/切换/删除/导入导出）
├── registry.rs           # registry.json + 8 个预设 Agent
├── db.rs                 # SQLite 存储
├── auto_sync.rs          # tokio 定时同步
├── onboarding.rs         # 首次初始化流程
└── types.rs              # 共享类型定义
```

## 安全说明

请务必使用**私有** Git 仓库。Agent 配置文件可能包含敏感信息（系统提示词、个人偏好等），公开仓库会导致泄露。

当前版本（MVP 阶段）的已知安全限制：

- **PAT 明文存储**：Git PAT 目前明文存在本地 SQLite 数据库中，尚未迁移到系统 keychain。后续版本计划迁移。
- **CSP 未启用**：Tauri 的 CSP 配置当前为 `null`，未启用内容安全策略。
- **人格包导入**：导入 `.zip` 人格包时未对 zip 内路径做穿越校验（潜在 zip slip 风险），且人格文件本质是 AI 系统提示词，恶意人格包可能包含 prompt 注入——导入前请务必审查 diff 预览。
- **路径穿越防护**：文件预览接口已校验 `..` 路径组件，防止目录穿越。

## 开发

```bash
# 类型检查
npm run typecheck

# 后端单元测试
cd src-tauri && cargo test
```

### 版本规范

遵循语义化版本（SemVer）。版本号唯一来源是 `src-tauri/tauri.conf.json` 的 `version` 字段，`package.json` 保持同步。

## 贡献

欢迎提 Issue 反馈 bug 或建议功能，也欢迎提 Pull Request。提 PR 前请阅读 [CONTRIBUTING.md](./CONTRIBUTING.md)，并确保：

1. `npm run typecheck` 通过
2. `cargo test` 通过
3. 遵循现有代码风格

## 许可证

[MIT License](./LICENSE)
