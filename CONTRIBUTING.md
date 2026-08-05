# 贡献指南

欢迎为 AgentSync 贡献代码！无论是修复 bug、新增功能、完善文档还是提交 Issue 反馈问题，都非常欢迎。

## 开发环境

**前置要求**：

- [Node.js](https://nodejs.org/) ≥ 18
- [Rust](https://www.rust-lang.org/) ≥ 1.77.2
- 系统 C++ 构建工具
  - Windows：Visual Studio Build Tools
  - macOS：Xcode Command Line Tools
  - Linux：`build-essential` 等

**拉起项目**：

```bash
# 克隆仓库
git clone https://github.com/Dawni233/AgentSync.git
cd AgentSync

# 安装前端依赖
npm install

# 开发模式（启动 Tauri 开发服务器，热重载）
npm run tauri dev
```

## 项目结构概览

- `src/` -- 前端（Vue 3 + TypeScript + Pinia）
  - `views/` -- 三个页面：Dashboard / Personalities / Settings
  - `components/` -- ConflictDialog / Onboarding / SyncStatusBadge
  - `composables/` -- useSync / usePersonalities / useToast
  - `stores/` -- Pinia stores（agents / settings / sync）
  - `types/` -- 前后端共享类型
  - `utils/diff.ts` -- 行级 diff（LCS 算法）
- `src-tauri/src/` -- 后端（Rust + Tauri 2）
  - `lib.rs` -- Tauri 命令注册 + 应用入口 + 系统托盘
  - `sync_engine.rs` -- 同步状态机
  - `git_sync.rs` -- git2 封装（clone/pull/commit/push）
  - `file_mapper.rs` -- glob 匹配 + 双向原子拷贝 + 快照回滚
  - `persona.rs` -- 人格管理（保存/切换/删除/导入导出）
  - `registry.rs` -- registry.json + 预设 Agent
  - `db.rs` -- SQLite 存储
  - `auto_sync.rs` -- tokio 定时同步
  - `onboarding.rs` -- 首次初始化流程
  - `types.rs` -- 共享类型定义

更详细的工作原理（同步状态机、数据存储布局）请参阅 [README.md](./README.md)。

## 提交前检查清单

提 PR 前，请在本地完成以下检查并确认通过：

1. `npm run typecheck` 通过（前端 TypeScript 类型检查）
2. `cd src-tauri && cargo test` 通过（后端单元测试）
3. `cd src-tauri && cargo fmt --check` 通过（Rust 代码格式）
4. `cd src-tauri && cargo clippy` 无警告

## 代码风格

### Rust

- 用 `cargo fmt` 自动格式化
- 用 `cargo clippy` 检查，无警告
- 错误处理用 `thiserror` 定义领域错误类型，避免 `unwrap()` / `expect()` 出现在非测试代码中
- 文件级原子写入（先写临时文件再 rename），Agent 级原子性（全部写暂存区成功才覆盖）

### TypeScript / Vue

- 用 2 空格缩进
- Vue 单文件组件用 `<script setup lang="ts">`
- 优先用 Pinia store 管理跨组件状态，避免直接 prop drilling
- 前后端共享类型定义在 `src/types/index.ts`，保持与 `src-tauri/src/types.rs` 对齐

### 设计系统

项目自研 "Atelier Console" 设计系统，使用离线字体（Bricolage Grotesque / IBM Plex），不依赖任何 UI 组件库。新增 UI 应遵循现有视觉风格，配色与排版见 `src/styles.css`。

## 提交规范

提交信息（commit message）使用以下前缀：

- `feat:` 新增功能
- `fix:` 修复 bug
- `docs:` 文档变更
- `refactor:` 重构（不改变功能）
- `chore:` 构建 / 工具 / 杂项
- `test:` 测试相关
- `release:` 发版

示例：`feat(persona): 支持人格包批量导入`

## 提交流程

1. Fork 本仓库并创建特性分支：`git checkout -b feat/your-feature`
2. 提交若干 commit，确保每条 commit 信息符合上述规范
3. 推送到你的 Fork，并向 `main` 分支发起 Pull Request
4. PR 描述中说明：改动目的、改动内容、是否影响现有行为、是否需要更新文档
5. 等待 CI 检查通过与维护者 review

## 版本规范

遵循语义化版本（SemVer）：`MAJOR.MINOR.PATCH`。版本号唯一来源是 `src-tauri/tauri.conf.json` 的 `version` 字段，`package.json` 与 `src-tauri/Cargo.toml` 保持同步。

- 不兼容的 API / 数据格式变更升 MAJOR
- 新增功能升 MINOR
- Bug 修复升 PATCH

## 行为准则

参与本项目的所有贡献者需遵守 [CODE_OF_CONDUCT.md](./CODE_OF_CONDUCT.md)。

## 安全相关

如果你发现安全漏洞，请按 [SECURITY.md](./SECURITY.md) 中的流程私下上报，**不要**在公开 Issue 中讨论。
