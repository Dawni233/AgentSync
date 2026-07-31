# AgentSync v0.2.x 缺口补全 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 闭环 Personalities 视图文件预览（真实数据 + 行级 diff）、消除 L2 手动合并选项的误导、清理死代码与残留依赖。

**Architecture:** 后端新增单个 IPC `read_persona_file`（核心逻辑为 `persona.rs` 纯函数，`lib.rs` 薄封装 + spawn_blocking），返回人格内容与本地内容；前端用纯 JS LCS 行级 diff 计算差异，md 走 `renderDoc` 渲染、json/toml 退化纯文本带行号。零新增依赖。L2 去误导仅改 UI；死代码清理删文件 + 卸 naive-ui。

**Tech Stack:** Rust 2021（git2/globset/shellexpand/walkdir，dev: tempfile）、Vue 3.5 + TypeScript 5.7 + Tauri 2.11、纯手写组件（无 UI 库）。

**Spec:** `docs/superpowers/specs/2026-07-31-gap-completion-design.md`

---

## 文件结构

| 文件 | 操作 | 职责 |
|---|---|---|
| `src-tauri/src/types.rs` | 修改 | 新增 `PersonaFileContent` 类型 |
| `src-tauri/src/persona.rs` | 修改 | 新增 `read_persona_file` 纯函数 + tests 模块 |
| `src-tauri/src/lib.rs` | 修改 | 新增 `read_persona_file` 命令 + 注册到 invoke_handler |
| `src/types/index.ts` | 修改 | 镜像 `PersonaFileContent` 接口 |
| `src/utils/diff.ts` | 创建 | `lineDiff` 纯函数（LCS 行级 diff） |
| `src/views/Personalities.vue` | 修改 | 删 `sampleContent`，接真实 IPC，接 diff.ts |
| `src/components/ConflictDialog.vue` | 修改 | L2 手动合并选项灰显去误导 |
| `src/components/AgentSidebar.vue` | 删除 | 死代码 |
| `src/components/AgentCard.vue` | 删除 | 死代码 |
| `src/components/PersonalityList.vue` | 删除 | 死代码 |
| `src/components/FilePreview.vue` | 删除 | 死代码 |
| `src/composables/useAgents.ts` | 删除 | stub |
| `package.json` | 修改 | 卸载 naive-ui |

---

## Task 1: 后端类型 `PersonaFileContent`

**Files:**
- Modify: `src-tauri/src/types.rs`（Persona 区域，约第 152 行 `Persona` 结构体之后）

- [ ] **Step 1: 新增 `PersonaFileContent` 结构体**

在 `src-tauri/src/types.rs` 的 `Persona` 结构体定义之后（第 152 行 `}` 之后）追加：

```rust
/// 人格文件预览内容（read_persona_file 返回值）
///
/// 人格快照内容与本地配置目录对应文件内容配对返回，
/// 供前端计算行级 diff。二进制文件两 content 均为 None。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersonaFileContent {
    /// 人格快照中的文件内容（UTF-8）；不存在/编码异常/二进制时为 None
    pub persona_content: Option<String>,
    /// 本地配置目录对应文件内容；不存在/编码异常/二进制时为 None
    pub local_content: Option<String>,
    /// 是否二进制文件（含 0x00 字节）
    pub is_binary: bool,
}
```

- [ ] **Step 2: 验证编译**

Run: `cd src-tauri && cargo check 2>&1 | tail -5`
Expected: 无错误（`PersonaFileContent` 暂未被使用，但 `derive` 的 `Serialize`/`Deserialize` 已在文件顶部导入，参考 `Persona` 结构）。

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/types.rs
git commit -m "feat(types): 新增 PersonaFileContent 类型"
```

---

## Task 2: 后端纯函数 `persona::read_persona_file`（TDD）

**Files:**
- Modify: `src-tauri/src/persona.rs`（新增函数 + tests 模块）

参考：`list_personalities`（同文件第 23 行起）用 `repo_path` + `agent_id` 拼人格目录；`file_mapper::expand_tilde`（`file_mapper.rs:64`）展开 `~`；`AgentConfig.config_dir` 存本地配置目录路径。

- [ ] **Step 1: 在 persona.rs 顶部补充 import**

`persona.rs` 第 12-18 行已有 imports。在 `use crate::types::Persona;`（第 15 行）下方追加：

```rust
use crate::types::{Persona, PersonaFileContent, AgentConfig};
```

（即将第 15 行的 `use crate::types::Persona;` 替换为上面这行，避免重复 import。）

- [ ] **Step 2: 编写失败的测试**

在 `persona.rs` 文件末尾追加 tests 模块：

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::AgentConfig;
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn make_config(config_dir: &str) -> AgentConfig {
        AgentConfig {
            id: "test-agent".into(),
            display_name: "Test".into(),
            config_dir: config_dir.into(),
            sync_files: vec!["SOUL.md".into(), "memory/**".into()],
            exclude_files: vec![],
            accent_color: Some("#5B4FE9".into()),
        }
    }

    fn write_file(base: &Path, rel: &str, content: &str) {
        let path = base.join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, content).unwrap();
    }

    #[test]
    fn read_persona_file_both_exist() {
        let repo = TempDir::new().unwrap();
        let local = TempDir::new().unwrap();
        // 仓库结构: repo/test-agent/work-mode/SOUL.md
        let persona_dir = repo.path().join("test-agent").join("work-mode");
        std::fs::create_dir_all(&persona_dir).unwrap();
        write_file(repo.path(), "test-agent/work-mode/SOUL.md", "persona version");

        // 本地结构: local/SOUL.md
        write_file(local.path(), "SOUL.md", "local version");

        let config = make_config(local.path().to_str().unwrap());
        let result = read_persona_file(
            repo.path(),
            &config,
            "work-mode",
            "SOUL.md",
        )
        .unwrap();

        assert!(!result.is_binary);
        assert_eq!(result.persona_content.as_deref(), Some("persona version"));
        assert_eq!(result.local_content.as_deref(), Some("local version"));
    }

    #[test]
    fn read_persona_file_local_missing() {
        let repo = TempDir::new().unwrap();
        let local = TempDir::new().unwrap();
        let persona_dir = repo.path().join("test-agent").join("work-mode");
        std::fs::create_dir_all(&persona_dir).unwrap();
        write_file(repo.path(), "test-agent/work-mode/SOUL.md", "only in persona");

        let config = make_config(local.path().to_str().unwrap());
        let result = read_persona_file(
            repo.path(),
            &config,
            "work-mode",
            "SOUL.md",
        )
        .unwrap();

        assert!(!result.is_binary);
        assert_eq!(result.persona_content.as_deref(), Some("only in persona"));
        assert_eq!(result.local_content, None);
    }

    #[test]
    fn read_persona_file_binary_detected() {
        let repo = TempDir::new().unwrap();
        let local = TempDir::new().unwrap();
        let persona_dir = repo.path().join("test-agent").join("work-mode");
        std::fs::create_dir_all(&persona_dir).unwrap();
        // 写入含 0x00 字节的二进制内容
        std::fs::write(
            repo.path().join("test-agent/work-mode/blob.bin"),
            [0x42, 0x00, 0x43],
        )
        .unwrap();

        let config = make_config(local.path().to_str().unwrap());
        let result = read_persona_file(
            repo.path(),
            &config,
            "work-mode",
            "blob.bin",
        )
        .unwrap();

        assert!(result.is_binary);
        assert_eq!(result.persona_content, None);
        assert_eq!(result.local_content, None);
    }

    #[test]
    fn read_persona_file_rejects_path_traversal() {
        let repo = TempDir::new().unwrap();
        let local = TempDir::new().unwrap();
        let config = make_config(local.path().to_str().unwrap());

        let result = read_persona_file(
            repo.path(),
            &config,
            "work-mode",
            "../../../etc/passwd",
        );

        assert!(result.is_err());
    }

    #[test]
    fn read_persona_file_persona_missing() {
        let repo = TempDir::new().unwrap();
        let local = TempDir::new().unwrap();
        let persona_dir = repo.path().join("test-agent").join("work-mode");
        std::fs::create_dir_all(&persona_dir).unwrap();
        // 人格目录存在但文件不存在；本地有文件
        write_file(local.path(), "SOUL.md", "local only");

        let config = make_config(local.path().to_str().unwrap());
        let result = read_persona_file(
            repo.path(),
            &config,
            "work-mode",
            "SOUL.md",
        )
        .unwrap();

        assert!(!result.is_binary);
        assert_eq!(result.persona_content, None);
        assert_eq!(result.local_content.as_deref(), Some("local only"));
    }
}
```

> **注意 `AgentConfig` 字段**：实现前需先读 `src-tauri/src/types.rs` 中 `AgentConfig` 的真实字段定义，核对 `make_config` 中的字段名与顺序是否一致。若字段不同，按真实定义调整 `make_config`。

- [ ] **Step 3: 运行测试确认失败**

Run: `cd src-tauri && cargo test --lib persona::tests 2>&1 | tail -15`
Expected: 编译失败，`read_persona_file` 未定义。

- [ ] **Step 4: 实现 `read_persona_file` 纯函数**

在 `persona.rs` 中（`list_personalities` 函数之后，`save_personality` 之前，约第 60 行附近）插入：

```rust
/// 读取人格文件内容及其对应的本地文件内容
///
/// 用于 Personalities 视图文件预览。`file_path` 为相对 agent 目录的路径
/// （如 "SOUL.md"、"memory/chat.md"），人格目录与本地 config_dir 同构，
/// 直接 join 相对路径即可定位本地文件。
///
/// - 二进制文件（含 0x00 字节）：`is_binary=true`，两 content 均为 None
/// - 任一文件不存在/编码异常：对应 content 为 None，不影响另一个
/// - 路径含 `..`：返回 Err（防目录穿越）
pub fn read_persona_file(
    repo_path: &Path,
    agent_config: &AgentConfig,
    persona_name: &str,
    file_path: &str,
) -> AppResult<PersonaFileContent> {
    // 路径安全：拒绝含 .. 的路径（防目录穿越）
    if file_path.split('/').any(|c| c == "..") || file_path.split('\\').any(|c| c == "..") {
        return Err(AppError::Config(format!(
            "非法文件路径 '{}': 含 .. 组件",
            file_path
        )));
    }

    let persona_path = repo_path
        .join(&agent_config.id)
        .join(persona_name)
        .join(file_path);
    let local_dir = file_mapper::expand_tilde(&agent_config.config_dir)?;
    let local_path = PathBuf::from(&local_dir).join(file_path);

    // 读取并检测二进制
    let read_text = |path: &Path| -> Option<String> {
        let bytes = fs::read(path).ok()?;
        if bytes.contains(&0u8) {
            return None; // 二进制
        }
        String::from_utf8(bytes).ok()
    };

    // 先读 bytes 判断二进制（任一为二进制则整体标记）
    let persona_bytes = fs::read(&persona_path).unwrap_or_default();
    let local_bytes = fs::read(&local_path).unwrap_or_default();
    let is_binary = persona_bytes.contains(&0u8) || local_bytes.contains(&0u8);

    if is_binary {
        return Ok(PersonaFileContent {
            persona_content: None,
            local_content: None,
            is_binary: true,
        });
    }

    Ok(PersonaFileContent {
        persona_content: read_text(&persona_path),
        local_content: read_text(&local_path),
        is_binary: false,
    })
}
```

- [ ] **Step 5: 运行测试确认通过**

Run: `cd src-tauri && cargo test --lib persona::tests 2>&1 | tail -15`
Expected: 5 个测试全部 PASS。

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/persona.rs
git commit -m "feat(persona): 新增 read_persona_file 纯函数 + 单测"
```

---

## Task 3: 后端 IPC 命令注册

**Files:**
- Modify: `src-tauri/src/lib.rs`（新增命令函数 + 注册到 invoke_handler）

参考：`list_personalities`（第 554 行）的薄封装模式；`list_tracked_files`（第 186 行）从 db 取 config 的模式。

- [ ] **Step 1: 在 lib.rs 新增命令函数**

在 `list_personalities` 命令（第 554-556 行）之后插入新命令：

```rust
/// 读取人格文件内容及其对应的本地文件内容（用于 Personalities 视图预览）
#[tauri::command]
fn read_persona_file(
    state: tauri::State<'_, AppState>,
    agent_id: String,
    persona_name: String,
    file_path: String,
) -> Result<PersonaFileContent, String> {
    let config = state
        .db
        .list_agents()
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|c| c.id == agent_id)
        .ok_or_else(|| format!("agent '{}' 未注册", agent_id))?;
    let repo_path = state.repo_path.clone();
    tauri::async_runtime::spawn_blocking(move || {
        persona::read_persona_file(&repo_path, &config, &persona_name, &file_path)
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())
}
```

- [ ] **Step 2: 在 lib.rs 顶部 import 新类型**

`lib.rs` 第 20 行现有：
```rust
use types::{Agent, AgentConfig, Persona, Settings, SyncResult, SyncStatus};
```
改为（在 `Persona` 之后插入 `PersonaFileContent`）：
```rust
use types::{Agent, AgentConfig, Persona, PersonaFileContent, Settings, SyncResult, SyncStatus};
```

- [ ] **Step 3: 注册到 invoke_handler**

在 `lib.rs` 的 `invoke_handler` 宏中（第 858 行 `list_personalities,` 附近），在 `list_personalities,` 之后追加一行：

```rust
            read_persona_file,
```

- [ ] **Step 4: 验证编译**

Run: `cd src-tauri && cargo check 2>&1 | tail -5`
Expected: 无错误。

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/lib.rs
git commit -m "feat(ipc): 注册 read_persona_file 命令"
```

---

## Task 4: 前端类型镜像 `PersonaFileContent`

**Files:**
- Modify: `src/types/index.ts`（Persona 区域，第 82 行 `Persona` 接口之后）

- [ ] **Step 1: 新增接口**

在 `src/types/index.ts` 的 `Persona` 接口之后追加：

```typescript
/** 人格文件预览内容（read_persona_file 返回值） */
export interface PersonaFileContent {
  /** 人格快照中的文件内容；不存在/编码异常/二进制时为 null */
  personaContent: string | null
  /** 本地配置目录对应文件内容；不存在/编码异常/二进制时为 null */
  localContent: string | null
  /** 是否二进制文件（含 0x00 字节） */
  isBinary: boolean
}
```

- [ ] **Step 2: 验证类型检查**

Run: `npm run typecheck 2>&1 | tail -5`
Expected: 无错误。

- [ ] **Step 3: Commit**

```bash
git add src/types/index.ts
git commit -m "feat(types): 镜像 PersonaFileContent 前端类型"
```

---

## Task 5: 前端 `lineDiff` 纯函数

**Files:**
- Create: `src/utils/diff.ts`

> **测试策略说明**：项目当前无前端测试框架（`package.json` scripts 仅有 build/typecheck）。spec 第 12 节已记录默认不引入 vitest，`lineDiff` 靠手动验收 + 类型检查覆盖。若引入 vitest 是独立决策，不在本计划范围。

- [ ] **Step 1: 创建 `src/utils/diff.ts`**

```typescript
/** 行级 diff 结果行 */
export type DiffLine = { type: 'same' | 'add' | 'del'; text: string }

/**
 * 行级 diff（基于 LCS 动态规划）。
 * @param a 基准侧（人格快照内容），对应 diff 中的 `-` 行
 * @param b 对照侧（本地内容），对应 diff 中的 `+` 行
 *
 * 边界：
 * - a 与 b 都为空 -> 返回 []
 * - a 为空 -> 全部为 add 行
 * - b 为空 -> 全部为 del 行
 * - a === b -> 全部为 same 行
 */
export function lineDiff(a: string, b: string): DiffLine[] {
  const linesA = a.length === 0 ? [] : a.split('\n')
  const linesB = b.length === 0 ? [] : b.split('\n')

  const m = linesA.length
  const n = linesB.length

  // dp[i][j] = linesA[0..i) 与 linesB[0..j) 的 LCS 长度
  const dp: number[][] = Array.from({ length: m + 1 }, () =>
    new Array(n + 1).fill(0)
  )
  for (let i = 1; i <= m; i++) {
    for (let j = 1; j <= n; j++) {
      if (linesA[i - 1] === linesB[j - 1]) {
        dp[i][j] = dp[i - 1][j - 1] + 1
      } else {
        dp[i][j] = Math.max(dp[i - 1][j], dp[i][j - 1])
      }
    }
  }

  // 回溯生成 diff 序列
  const result: DiffLine[] = []
  let i = m
  let j = n
  while (i > 0 || j > 0) {
    if (i > 0 && j > 0 && linesA[i - 1] === linesB[j - 1]) {
      result.push({ type: 'same', text: linesA[i - 1] })
      i--
      j--
    } else if (j > 0 && (i === 0 || dp[i][j - 1] >= dp[i - 1][j])) {
      result.push({ type: 'add', text: linesB[j - 1] })
      j--
    } else {
      result.push({ type: 'del', text: linesA[i - 1] })
      i--
    }
  }
  result.reverse()
  return result
}
```

- [ ] **Step 2: 验证类型检查**

Run: `npm run typecheck 2>&1 | tail -5`
Expected: 无错误。

- [ ] **Step 3: Commit**

```bash
git add src/utils/diff.ts
git commit -m "feat(utils): 新增 lineDiff 行级 diff 纯函数"
```

---

## Task 6: 前端 Personalities.vue 接真实数据

**Files:**
- Modify: `src/views/Personalities.vue`

这是本计划最大的任务。改动点：删 `sampleContent`、接 IPC、接 `lineDiff`、重写 `previewHtml`/`renderDiff`。

- [ ] **Step 1: 补充 import**

修改 `src/views/Personalities.vue` 第 2-6 行的 import 区块。在现有 import 基础上追加：

```typescript
import { invoke } from '@tauri-apps/api/core'
import { lineDiff, type DiffLine } from '@/utils/diff'
import type { Persona, PersonaFileContent } from '@/types'
```

注意：原第 6 行 `import type { Persona } from '@/types'` 要替换为上面第三行（合并 Persona 与 PersonaFileContent）。

- [ ] **Step 2: 新增文件内容状态与加载逻辑**

在第 27 行 `const selMode = ref<...>('render')` 之后追加：

```typescript
const fileContent = ref<PersonaFileContent | null>(null)
const loadingFile = ref(false)
```

- [ ] **Step 3: 新增加载函数**

在 `selectFile` 函数（第 237-239 行）之后追加 `loadFileContent` 函数，并改写 `selectFile` 触发它：

```typescript
function selectFile(file: string) {
  selFile.value = file
  loadFileContent()
}

async function loadFileContent() {
  if (!selected.value || !selFile.value) {
    fileContent.value = null
    return
  }
  loadingFile.value = true
  try {
    fileContent.value = await invoke<PersonaFileContent>('read_persona_file', {
      agentId: selected.value.agentId,
      personaName: selected.value.name,
      filePath: selFile.value,
    })
  } catch (e) {
    fileContent.value = null
    showToast(`读取文件失败: ${e}`)
  } finally {
    loadingFile.value = false
  }
}
```

注意：`showToast` 已在第 5 行 import（`import { showToast } from '@/composables/useToast'`），直接复用。

- [ ] **Step 4: 选中人格时也触发加载**

修改 `selectPersona` 函数（第 230-235 行），在末尾 `selFile.value = ...` 之后调用加载。因为 `selFile.value =` 赋值不会触发 `selectFile`，需手动调用：

将原：
```typescript
function selectPersona(agentId: string, name: string) {
  selected.value = { agentId, name }
  const p = (grouped.value[agentId] || []).find((x) => x.name === name) || null
  selectedPersona.value = p
  selFile.value = p && p.files.length ? p.files[0] : null
}
```

改为：
```typescript
function selectPersona(agentId: string, name: string) {
  selected.value = { agentId, name }
  const p = (grouped.value[agentId] || []).find((x) => x.name === name) || null
  selectedPersona.value = p
  selFile.value = p && p.files.length ? p.files[0] : null
  loadFileContent()
}
```

- [ ] **Step 5: 删除 `sampleContent` 函数**

删除第 110-143 行整个 `sampleContent` 函数。

- [ ] **Step 6: 重写 `renderDiff`**

将第 191-200 行 `renderDiff` 函数替换为基于 `fileContent` 的实现：

```typescript
function renderDiff(): string {
  const fc = fileContent.value
  if (!fc || fc.isBinary) {
    return '<div class="empty"><div class="empty__icon">⛔</div><div class="empty__title">二进制文件无法 diff</div></div>'
  }
  const persona = fc.personaContent ?? ''
  const local = fc.localContent ?? ''
  if (!persona && !local) {
    return '<div class="empty"><div class="empty__icon">📄</div><div class="empty__title">文件内容为空</div></div>'
  }
  if (!persona) {
    return '<div class="diff__h">本地新增文件</div>'
  }
  const diff: DiffLine[] = lineDiff(persona, local)
  if (!fc.localContent) {
    return '<div class="diff__h">本地无此文件（人格独有）</div>'
  }
  let html = `<div class="diff__h">@@ 行差异 · ${diff.length} 行 @@</div>`
  for (const d of diff) {
    const sign = d.type === 'add' ? '+' : d.type === 'del' ? '-' : ' '
    const cls = d.type === 'add' ? 'diff__add' : d.type === 'del' ? 'diff__del' : ''
    html += `<div class="diff__row ${cls}"><span class="diff__sign">${sign}</span><span class="diff__txt">${esc(d.text)}</span></div>`
  }
  return html
}
```

- [ ] **Step 7: 重写 `previewHtml` computed**

将第 202-220 行 `previewHtml` computed 替换为：

```typescript
const previewHtml = computed(() => {
  if (loadingFile.value) {
    return '<div class="empty"><div class="empty__icon">⏳</div><div class="empty__title">加载中…</div></div>'
  }
  if (!selectedPersona.value || !selFile.value) {
    return '<div class="empty"><div class="empty__icon">📄</div><div class="empty__title">选择文件预览</div></div>'
  }
  const fc = fileContent.value
  if (!fc) {
    return '<div class="empty"><div class="empty__icon">📄</div><div class="empty__title">无内容</div></div>'
  }
  if (fc.isBinary) {
    return '<div class="empty"><div class="empty__icon">⛔</div><div class="empty__title">二进制文件无法预览</div><div>请选择其他文件</div></div>'
  }
  const file = selFile.value

  // diff Tab
  if (selMode.value === 'diff') {
    return `<div class="diff">${renderDiff()}</div>`
  }

  // 内容取 persona 优先，fallback local
  const content = fc.personaContent ?? fc.localContent ?? ''
  const isMd = /\.md$/i.test(file)

  // source Tab：纯文本带行号
  if (selMode.value === 'source') {
    if (!content) {
      return '<div class="empty"><div class="empty__icon">📄</div><div class="empty__title">文件内容为空</div></div>'
    }
    const lines = content.split('\n')
    return `<div class="code">${lines
      .map((l, i) => `<div><span class="ln">${i + 1}</span>${highlight(file, l)}</div>`)
      .join('')}</div>`
  }

  // render Tab：仅 md 渲染
  if (isMd) {
    return `<div class="doc">${renderDoc(content.split('\n'))}</div>`
  }
  return '<div class="empty"><div class="empty__icon">📄</div><div class="empty__title">无渲染视图</div><div>该文件类型暂不支持渲染，请切换到「源码」</div></div>'
})
```

- [ ] **Step 8: 验证类型检查与构建**

Run: `npm run typecheck 2>&1 | tail -10`
Expected: 无错误。

Run: `npm run build 2>&1 | tail -10`
Expected: 构建成功，无未使用变量/import 警告（`sampleContent` 已删，`renderDiff` 参数已改，注意检查是否有遗漏引用）。

- [ ] **Step 9: Commit**

```bash
git add src/views/Personalities.vue
git commit -m "feat(personalities): 文件预览接入真实数据 + 行级 diff"
```

---

## Task 7: L2 手动合并选项去误导

**Files:**
- Modify: `src/components/ConflictDialog.vue`（第 94-100 行）

- [ ] **Step 1: 修改手动合并选项**

将 `src/components/ConflictDialog.vue` 第 94-100 行：

```html
            <div class="opt" @click="resolve('L2ManualMerge')">
              <div class="opt__radio" />
              <div>
                <div class="opt__t">手动合并</div>
                <div class="opt__d">打开内置编辑器，逐文件合并（Phase 4 实现）</div>
              </div>
            </div>
```

替换为：

```html
            <div class="opt opt--disabled" aria-disabled="true">
              <div class="opt__radio" />
              <div>
                <div class="opt__t">手动合并</div>
                <div class="opt__d">手动合并编辑器（开发中，暂不可用）</div>
              </div>
            </div>
```

- [ ] **Step 2: 新增禁用样式**

在 `src/components/ConflictDialog.vue` 的 `<style scoped>` 区块中追加（若 `opt--disabled` 样式已存在则跳过）：

```css
.opt--disabled {
  opacity: 0.45;
  cursor: not-allowed;
}
.opt--disabled .opt__radio {
  background: var(--surface-2);
}
```

- [ ] **Step 3: 验证类型检查与构建**

Run: `npm run build 2>&1 | tail -10`
Expected: 构建成功。

- [ ] **Step 4: Commit**

```bash
git add src/components/ConflictDialog.vue
git commit -m "fix(conflict): L2 手动合并选项灰显去误导"
```

---

## Task 8: 清理死代码与残留依赖

**Files:**
- Delete: `src/components/AgentSidebar.vue`, `src/components/AgentCard.vue`, `src/components/PersonalityList.vue`, `src/components/FilePreview.vue`
- Delete: `src/composables/useAgents.ts`
- Modify: `package.json`（卸载 naive-ui）

- [ ] **Step 1: 删除死代码组件**

```bash
rm src/components/AgentSidebar.vue src/components/AgentCard.vue src/components/PersonalityList.vue src/components/FilePreview.vue
```

- [ ] **Step 2: 删除 stub composable**

```bash
rm src/composables/useAgents.ts
```

- [ ] **Step 3: 卸载 naive-ui**

```bash
npm uninstall naive-ui
```

- [ ] **Step 4: 验证无失效引用**

Run: `npm run typecheck 2>&1 | tail -10`
Expected: 无错误（已核实 4 组件 + useAgents 均零引用，naive-ui 在 src 下零引用）。

Run: `npm run build 2>&1 | tail -10`
Expected: 构建成功。

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "chore: 清理死代码组件与 naive-ui 残留依赖

删除：AgentSidebar/AgentCard/PersonalityList/FilePreview 组件、
useAgents stub composable；卸载 naive-ui（src 下零引用）。"
```

---

## Task 9: 手动验收

**Files:** 无（验证性任务）

> 此任务需在 Tauri dev 环境运行应用，验证端到端功能。

- [ ] **Step 1: 启动应用**

Run: `npm run tauri dev`（在另一个终端，或后台运行）
Expected: 应用启动，进入主界面。

- [ ] **Step 2: 缺口 A - 渲染/源码/diff 三 Tab 真实数据**

进入「人格管理」视图，选 WorkBuddy 的某个人格：
- 选中 SOUL.md（或任一 .md 文件）-> 渲染 Tab 显示真实 markdown 渲染（标题/列表/引用等结构，非假数据）
- 切到源码 Tab -> 显示真实文件内容带行号
- 切到差异 Tab -> 若本地 SOUL.md 与人格快照不同，显示红绿 diff 行；若相同，显示全 same 行（` ` 符号）

- [ ] **Step 3: 缺口 A - json 文件纯文本**

选 Claude Code 人格，选中 settings.json：
- 源码 Tab 显示真实 JSON 纯文本带行号
- 渲染 Tab 提示"该文件类型暂不支持渲染，请切换到「源码」"

- [ ] **Step 4: 缺口 A - 本地不存在文件**

选中一个人格独有的文件（本地 config_dir 没有的文件）：
- 差异 Tab 显示"本地无此文件（人格独有）"提示，或全部 `+` 行

- [ ] **Step 5: 缺口 A - loading 态**

快速切换文件：预览区短暂显示"加载中…"，无旧数据残留。

- [ ] **Step 6: 缺口 B' - L2 去误导**

触发一次 L2 冲突（本地与远端同时修改同一文件后同步）：
- 冲突弹窗中"手动合并"选项灰显（opacity 低、cursor not-allowed）
- 鼠标点击无反应，不触发 resolve
- 描述文本为"手动合并编辑器（开发中，暂不可用）"

- [ ] **Step 7: 缺口 D - 清理验证**

- `src/components/` 不再有 AgentSidebar/AgentCard/PersonalityList/FilePreview
- `src/composables/` 不再有 useAgents.ts
- `package.json` dependencies 不再含 naive-ui
- 应用启动正常，Dashboard/Personalities/Settings 三视图功能正常

- [ ] **Step 8: 全量构建验证**

Run: `npm run build 2>&1 | tail -10` && `cd src-tauri && cargo check 2>&1 | tail -5`
Expected: 前后端构建均成功。

---

## 设计符合度对照（实现完成时复核）

| Spec 要求 | 对应 Task |
|---|---|
| 后端 `PersonaFileContent` 类型 | Task 1 |
| 后端 `persona::read_persona_file` 纯函数 + 单测 | Task 2 |
| 后端 IPC 命令 + 注册 | Task 3 |
| 前端 `PersonaFileContent` 类型镜像 | Task 4 |
| 前端 `lineDiff` 纯函数（LCS） | Task 5 |
| 前端 Personalities.vue 替换假数据 + 三 Tab 真实渲染 | Task 6 |
| 前端 L2 去误导（灰显 + 描述） | Task 7 |
| 前端死代码 + stub + naive-ui 清理 | Task 8 |
| 路径安全（`..` 拒绝） | Task 2（测试 + 实现） |
| 二进制检测 | Task 2（测试 + 实现） |
| 错误处理（文件不存在/编码/agent 未注册） | Task 2 + Task 6 |
| 手动验收清单 | Task 9 |

---

## 风险与备注

- **`AgentConfig` 字段核对**：Task 2 的 `make_config` 测试辅助函数假设了 `AgentConfig` 的字段。实现时必须先读 `src-tauri/src/types.rs` 中 `AgentConfig` 真实定义核对，字段名/顺序不同则调整 `make_config`。
- **前端无单测框架**：Task 5 的 `lineDiff` 靠类型检查 + Task 9 手动验收覆盖。若执行者倾向引入 vitest，需单独决策（不在本计划）。
- **`spawn_blocking` 与 `list_personalities` 不一致**：`list_personalities` 是同步命令，本计划的 `read_persona_file` 用 `spawn_blocking`（spec 要求）。两者风格不一致但可接受--读双文件 + expand_tilde 比 `list_personalities` 的单目录扫描略重，且与 `list_tracked_files`（也同步）相比多了 spawn 开销。若执行者认为过度，可改为同步，不影响功能。
- **`esc` 函数**：Task 6 的 `renderDiff` 与 `previewHtml` 依赖 `esc`（HTML 转义）。该函数已在 `Personalities.vue` 中存在（`sampleContent`/`renderDoc` 都用了），无需新增。
