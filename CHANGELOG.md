# 更新日志

本文件记录 AgentSync 的版本变更，遵循 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/) 格式，版本号遵循 [语义化版本](https://semver.org/lang/zh-CN/)。

## [Unreleased]

### 新增

- 开源治理文件：CONTRIBUTING.md、SECURITY.md、CODE_OF_CONDUCT.md、CHANGELOG.md
- GitHub Issue / PR 模板与 CI workflow
- 完善 package.json / Cargo.toml / tauri.conf.json 元信息字段
- `.gitignore` 补充 `.env*`、`src-tauri/gen/`、`coverage/` 规则

### 修复

- 同步 `Cargo.toml` 版本号至 0.2.1（此前为 0.1.0，与 package.json / tauri.conf.json 不一致）
- 修正 README 中 Releases 链接为 GitHub 绝对地址
- 移除 README 对未上传的 `AGENTS.md` 的引用

## [0.2.1] - 2026-07-31

### 修复

- 补上 `dialog:default` 权限，修复导入文件选择框不弹出的问题
- 移除人格页面的导出功能入口
- 移除 Dashboard 的模拟冲突功能
- 清理死代码组件与 naive-ui 残留依赖

## [0.2.0] - 2026-07-30

### 新增

- 人格（Persona）管理：保存 / 切换 / 删除命名快照
- 人格包导入导出（`.zip`，含 diff 预览与风险提示）
- 自动同步定时器（5 / 15 / 30 / 60 分钟）
- 开机自启动与系统托盘
- 两级冲突处理（L1 Git 历史 / L2 配置内容）
- 文件预览（渲染 / 源码 / 差异三种视图）
- 原子写入（文件级 + Agent 级）

## [0.1.0] - 2026-07-27

### 新增

- 首个可用版本
- 8 个预设 Agent（WorkBuddy / Claude Code / Cursor / Codex / ZCode / Qoder / OpenClaw / QwenPaw）
- Git 私有仓库同步（GitHub / Gitee，PAT 认证）
- 首次初始化向导
- 同步状态机驱动的核心引擎

[Unreleased]: https://github.com/Dawni233/AgentSync/compare/v0.2.1...HEAD
[0.2.1]: https://github.com/Dawni233/AgentSync/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/Dawni233/AgentSync/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/Dawni233/AgentSync/releases/tag/v0.1.0
