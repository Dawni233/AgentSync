# AgentSync v0.2.x 缺口补全设计

- **日期**：2026-07-31
- **阶段**：v0.2.0 之后的"补全现有功能缺口"阶段
- **范围**：不新增功能面，仅闭环已规划但未落地的能力 + 清理技术债
- **状态**：待实现

---

## 1. 背景与动机

AgentSync v0.2.0 已落地核心同步/人格/自动同步/托盘骨架并可运行。探索发现三类未闭环项：

| 项 | 现状 | 性质 |
|---|---|---|
| **A** | `Personalities.vue` 文件预览三 Tab（渲染/源码/差异）全部走 `sampleContent()` 假数据，后端缺读取接口 | 功能未闭环 |
| **B'** | `ConflictDialog.vue` 的 L2"手动合并"选项可点击，描述"打开内置编辑器（Phase 4 实现）"，但点击后空走 S4 当作"保留本地"提交，误导用户 | 误导风险 |
| **D** | 4 个死代码组件 + `useAgents.ts` stub + naive-ui 残留依赖 | 技术债 |

**明确不在本轮**：L2 合并编辑器实现、PAT 迁移 keychain、默认分支探测、README/CHANGELOG 补全、`Cargo.toml` 版本号同步。这些属于工程化/发布范畴，留待后续阶段。

---

## 2. 核心设计决策

### 2.1 缺口 A：人格文件预览真实化

**差异语义**：原型 HTML（`design/AgentSync-UI-Prototype.html` 第 538-547 行）只定义了差异视图的 CSS 样式（`.diff__row`/`.diff__del`/`.diff__add`，红绿行 + +/- 符号），未定义比较对象。经用户澄清确认：**差异 Tab 比较"人格快照"与"该 Agent 当前本地实际配置文件"**——即"切换到这个人格后本地会发生什么变化"的预演。

**IPC 架构**：采用单文件按需读取方案。新增一个后端命令 `read_persona_file`，选中文件时调用，返回该文件的人格内容与本地内容。理由：
- 与现有"选中文件 -> 预览"交互流一致
- 数据量小、响应快
- 与 `list_personalities`（返回文件列表）形成清晰两层结构：列表轻量、详情按需

**diff 计算位置**：放在前端，用纯 JS LCS 行级 diff。理由：
- 后端引入 `similar` crate 增加编译依赖和产物体积，只为 UI 预览服务，不划算
- 前端已有 `renderDiff` 骨架，行级 LCS 约 30 行纯 JS，零依赖
- diff 只在用户切到差异 Tab 时按需计算，放前端天然按需
- 符合"后端做 IO，前端做展示"的职责划分

**高亮范围**：只做 markdown 渲染 + json/toml 退化纯文本（带行号）。不做 yaml/ini。理由：经核查 `registry.rs` 预置 agent 的 syncFiles，实际文件类型为 markdown 压倒性多数（SOUL.md/IDENTITY.md/USER.md/CLAUDE.md/AGENTS.md/HEARTBEAT.md）、少量 json（settings.json/config.json/token_usage.json/exec-approvals.json）、1 个 toml（config.toml）。无 yaml、无 ini。按 AGENTS.md"不应对未来可能用到的场景"原则，不引入不存在的格式支持。

**不引入高亮库**：保持项目"纯手写、零运行时 UI 依赖"路线（naive-ui 即将在缺口 D 卸载）。复用前端已有的 `renderDoc`（markdown 渲染）和 `highlight`（正则高亮）骨架。

### 2.2 路径映射（关键验证项，已查清）

人格文件与本地文件的映射基于**同构相对路径**（`file_mapper.rs` 第 8-9、106-111 行）：

- 人格目录 `{repo}/{agent_id}/{persona_name}/` 与本地 `config_dir` 同构
- 两者的文件都用相对路径表示（如 `SOUL.md`、`memory/chat_2024.md`）
- 同一相对路径在两个目录下指向同一逻辑文件
- 本地文件读取：`expand_tilde(config_dir).join(file_path)` 即可，无需 glob 反查

### 2.3 文件级差异标注：不做

原型 HTML 的文件树（`.file-rail__list`）只是文件列表，无差异标注设计。差异仅在选中文件后的差异 Tab 体现。因此前端不需要预载本地文件列表，不需要文件级差异计算。这进一步简化设计——完全不需要 `list_tracked_files` 或本地文件枚举。

### 2.4 缺口 B'：去误导而非实现

L2 手动合并的完整编辑器不在本轮。仅做 UI 层去误导：
- 选项灰显不可点击
- 描述改为"开发中，暂不可用"
- 保留后端 `L2ManualMerge` 枚举成员（维持完备性，不删）

### 2.5 缺口 D：清理范围（已核实全部无引用）

| 清理项 | 核实结果 |
|---|---|
| `src/components/AgentSidebar.vue` | `src/` 下无引用 |
| `src/components/AgentCard.vue` | 无引用 |
| `src/components/PersonalityList.vue` | 无引用 |
| `src/components/FilePreview.vue` | 无引用 |
| `src/composables/useAgents.ts` | stub（`// TODO Phase 2`），无引用；实际逻辑在 `stores/agents.ts` |
| `naive-ui` 依赖 | `src/` 下零引用，仅 `package.json` 残留 |

---

## 3. 架构与数据流

### 3.1 缺口 A 数据流

```
前端选中文件
  -> invoke('read_persona_file', { agentId, personaName, filePath })
  -> 后端 spawn_blocking:
       路径安全校验（拒绝含 .. 的 file_path）
       persona_content = read_to_string(repo/agent_id/persona_name/file_path)
       config = db.list_agents().find(agent_id)
       local_path = expand_tilde(config.config_dir).join(file_path)
       local_content = read_to_string(local_path)
       is_binary = bytes 含 0x00
  <- { personaContent, localContent, isBinary }
  -> 前端按 selMode 渲染:
       source  -> md: renderDoc / json,toml,其他: 纯文本带行号
       render  -> md: renderDoc / 其他: 提示切到源码
       diff    -> lineDiff(personaContent, localContent) 行级 diff
```

### 3.2 职责划分

- **后端**：只做 IO（读两个文件 + 二进制检测 + 路径安全），零新增依赖
- **前端**：全部展示与 diff 计算，零新增依赖

### 3.3 结构骨架

本轮不新增视图/路由/目录。所有改动落在既有文件内部：
- 后端：`persona.rs`（新增函数）/`lib.rs`（注册命令）/`types.rs`（新增类型）
- 前端：`Personalities.vue`（替换假数据）/`ConflictDialog.vue`（去误导）/新增 `src/utils/diff.ts`

---

## 4. 后端改动（缺口 A）

### 4.1 新增 IPC 命令 `read_persona_file`

**分层**：核心逻辑放在 `src-tauri/src/persona.rs` 作为纯函数（便于单测），`lib.rs` 的命令仅作薄封装（取 state + spawn_blocking + 错误转字符串）。

位置：`src-tauri/src/lib.rs`（命令，与 `list_personalities` 同区域）+ `src-tauri/src/persona.rs`（核心函数）

```rust
// persona.rs - 核心纯函数
pub fn read_persona_file(
    repo_path: &Path,
    agent_config: &AgentConfig,
    persona_name: &str,
    file_path: &str,
) -> AppResult<PersonaFileContent>

// lib.rs - 命令薄封装
/// 读取人格文件内容及其对应的本地文件内容
///
/// 用于 Personalities 视图文件预览。file_path 为相对 agent 目录的路径
/// （如 "SOUL.md"、"memory/chat.md"），人格目录与本地 config_dir 同构。
#[tauri::command]
fn read_persona_file(
    state: tauri::State<'_, AppState>,
    agent_id: String,
    persona_name: String,
    file_path: String,
) -> Result<PersonaFileContent, String> {
    tauri::async_runtime::spawn_blocking(...) // 调 persona::read_persona_file
}
```

实现要点：
- 在 `tauri::async_runtime::spawn_blocking` 中执行（纯 fs 操作，与现有命令一致）
- **路径安全校验**：`file_path` 不得含 `..` 组件（防目录穿越），否则返回错误
- `persona_path = state.repo_path.join(&agent_id).join(&persona_name).join(&file_path)`
- 从 db 取 `AgentConfig`（与 `list_tracked_files` 同样的查找方式），`local_path = expand_tilde(&config.config_dir)?.join(&file_path)`
- 二进制检测：先 `fs::read` 拿 bytes，若含 `0x00` 字节视为二进制，`is_binary=true` 且两个 content 均为 null
- 文本读取：`String::from_utf8` 失败时 content 为 null（不阻塞另一个文件）
- 两个文件独立读取，任一不存在/异常不影响另一个

### 4.2 新增类型 `PersonaFileContent`

位置：`src-tauri/src/types.rs`（Persona 区域）

```rust
/// 人格文件预览内容（read_persona_file 返回值）
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

**不引入 diff 相关类型**——diff 完全在前端。不引入 `similar` 或任何新 crate。

### 4.3 注册命令

`lib.rs` 的 `invoke_handler` 宏中，在 `list_personalities` 附近添加 `read_persona_file`。

---

## 5. 前端改动（缺口 A）

### 5.1 类型同步（`src/types/index.ts`）

新增 `PersonaFileContent` 接口，与 Rust 结构镜像（camelCase）。

### 5.2 新增 `src/utils/diff.ts`

纯函数 LCS 行级 diff，便于单测：

```typescript
export type DiffLine = { type: 'same' | 'add' | 'del'; text: string }

/**
 * 行级 diff（基于 LCS）。
 * @param a 人格快照内容（基准，作为 - 侧）
 * @param b 本地内容（对照，作为 + 侧）
 */
export function lineDiff(a: string, b: string): DiffLine[]
```

经典 LCS DP 实现：
- 按 `\n` 分割两字符串为行数组
- DP 表计算 LCS
- 回溯生成 `same`/`add`/`del` 序列
- 空 a、空 b、全相同、全新增、全删除均需正确处理

### 5.3 `Personalities.vue` 改动

**删除**：
- `sampleContent()` 函数（第 110-143 行）
- `renderDiff` 中对 `sampleContent` 的依赖

**新增**：
- `selectedFileContent = ref<PersonaFileContent | null>(null)`
- 选中文件时（`onFileTreeClick` 或 `selFile` 的 watcher）调用 `invoke('read_persona_file', ...)`，结果存入 `selectedFileContent`；加 loading 态防止闪烁
- 引入 `lineDiff` from `@/utils/diff`

**`previewHtml` computed 改写**（数据源从 `sampleContent` 改为 `selectedFileContent`）：

| Tab | md 文件 | json/toml/其他 | 二进制 |
|---|---|---|---|
| source | 纯文本带行号（用 `persona_content`，复用现有 `highlight` 对 md 做轻量正则高亮可选） | 纯文本带行号（用 `persona_content`） | 提示"二进制文件无法预览" |
| render | `renderDoc` 渲染（用真实 `persona_content`） | 提示"该文件类型暂不支持渲染，请切换到「源码」" | 同上 |
| diff | `lineDiff(persona_content, local_content)` 渲染红绿行 | 同左 | 提示"二进制文件无法 diff" |

> 说明：source Tab 对所有文本文件统一为"纯文本带行号"（与现状一致），不按格式分派不同高亮逻辑；render Tab 才按格式分派（md 渲染 / 其他不支持）。这与现有 `previewHtml` 逻辑结构一致，仅替换数据源。

**diff Tab 边界**：
- `local_content` 为 null（本地无此文件）-> 全部显示为 `+` 行（新增）
- `persona_content` 为 null -> 异常态，提示"文件不存在"
- 二进制 -> 提示无法 diff

**loading 态**：文件切换到数据返回之间，预览区显示轻量 loading（避免用户看到旧数据或空白误以为无内容）。

---

## 6. 前端改动（缺口 B'）

### `src/components/ConflictDialog.vue`

当前第 94-100 行的 L2 手动合并选项可点击。改为：

- 选项容器加 `disabled` 类（视觉灰显），移除 `@click` 绑定或加 guard 使其不触发 `resolve('L2ManualMerge')`
- 描述文本改为"手动合并编辑器（开发中，暂不可用）"
- 不改 `ResolutionOption` 类型联合（保留 `'L2ManualMerge'` 成员）
- 不删后端 `L2ManualMerge` 枚举分支

---

## 7. 前端改动（缺口 D）

- 删除 `src/components/AgentSidebar.vue`、`AgentCard.vue`、`PersonalityList.vue`、`FilePreview.vue`
- 删除 `src/composables/useAgents.ts`
- `npm uninstall naive-ui`
- 检查删除后是否有失效 import（预期无，因已核实零引用）

---

## 8. 错误处理

| 场景 | 后端 | 前端 |
|---|---|---|
| 人格文件不存在 | `persona_content: None` | source/render/diff 提示"文件不存在" |
| 本地文件不存在（人格新增文件） | `local_content: None` | diff Tab 全部显示为 `+` 行 |
| 二进制文件（含 0x00） | `is_binary: true`，两 content 均 None | 三 Tab 均提示"二进制文件无法预览" |
| 路径含 `..` | 返回 Err | toast 提示"非法路径" |
| 读取异常（权限/编码） | 对应 content 为 None，不阻塞另一个 | 提示对应侧不可用 |
| agent_id 未注册 | 返回 Err | toast 提示 |

---

## 9. 测试

| 层 | 文件 | 用例 |
|---|---|---|
| 后端单测 | `src-tauri/src/persona.rs` 的 `#[cfg(test)]` 模块 | `persona::read_persona_file` 纯函数：正常读取 / 本地文件不存在 / 二进制检测 / `..` 路径拒绝 / agent 不存在 |
| 前端单测 | `src/utils/diff.test.ts` | `lineDiff` 全相同 / 全新增（a 空） / 全删除（b 空） / 部分修改 / 空行处理 |
| 手动验收 | - | 见第 10 节 |

> 注：前端目前无测试框架与单测先例。`diff.ts` 是纯函数，适合作为首个单测切入点，但需先确认是否引入 vitest。若不引入，则 `lineDiff` 通过手动验收覆盖。

---

## 10. 手动验收清单

### 缺口 A
- [ ] 选 WorkBuddy 某人格 -> 选中 SOUL.md -> 渲染 Tab 显示真实 markdown 渲染（非假数据）
- [ ] 同上 -> 源码 Tab 显示真实内容带行号
- [ ] 同上 -> 差异 Tab：若本地 SOUL.md 与人格不同，显示真实红绿 diff；若相同，显示全 same 行
- [ ] 选 Claude Code 人格 -> 选中 settings.json -> 源码 Tab 显示真实 JSON 纯文本（非高亮渲染）
- [ ] 选中本地不存在的文件（人格新增）-> 差异 Tab 全部显示为 `+` 行
- [ ] 选中二进制文件 -> 三 Tab 均提示"二进制文件无法预览"
- [ ] 切换文件时预览区有 loading 态，无旧数据残留

### 缺口 B'
- [ ] 触发 L2 冲突 -> 弹窗中"手动合并"选项灰显不可点击
- [ ] 描述文本为"手动合并编辑器（开发中，暂不可用）"

### 缺口 D
- [ ] `npm run build` 通过（无失效 import）
- [ ] `package.json` 不再含 naive-ui
- [ ] `src/components/` 不再有 4 个死代码组件
- [ ] `src/composables/` 不再有 useAgents.ts
- [ ] 应用启动正常，Personalities/Dashboard/Settings 视图功能正常

---

## 11. 设计符合度对照（AGENTS.md 要求）

| AGENTS.md 条款 | 本设计遵守情况 |
|---|---|
| 设计文档对照 | 原型 HTML 未定义差异语义，以用户澄清"人格 vs 本地"为准；UI 结构（文件树 + 三 Tab 预览）严格按原型，不新增视图/路由 |
| 简洁优先 | 零新依赖、零新视图；后端 +1 IPC +1 类型；前端删多于增（净减少文件数）；diff 30 行纯 JS 不引入库 |
| 精准修改 | 改动限定在既有文件内部，不顺手优化相邻代码；L2 去误导仅改 UI 不删后端枚举 |
| 目标驱动 | 缺口 A 验收=三 Tab 真实数据 + diff 正确；B'=选项灰显不可点；D=死代码清零 + 构建通过 |
| 版本与发布 | 本轮属 v0.2.x 补丁，按 SemVer 应升 PATCH（v0.2.1）。发版时需同步 `tauri.conf.json`/`package.json`/`Cargo.toml` 三处版本（注意：`Cargo.toml` 当前 0.1.0 未同步，本轮应一并修正为 0.2.1） |

---

## 12. 风险与备注

- **`Cargo.toml` 版本号长期未同步**（0.1.0 vs 0.2.0）：这是既有问题，非本轮引入。但本轮若发版为 v0.2.1，应顺手统一三处版本号（符合 AGENTS.md"版本号唯一来源"约定）。若不发版，则仅记录不改。
- **前端无单测框架**：`lineDiff` 是纯函数适合单测，但引入 vitest 是独立决策。spec 默认不引入，靠手动验收覆盖；若用户倾向引入 vitest，可在实现计划阶段调整。
- **`Personalities.vue` 已 547 行**：本轮替换 `sampleContent` 不会显著增减行数。若实现时发现预览逻辑膨胀，可考虑抽 `FilePreviewPanel.vue` 组件——但这是实现期决策，不在本 spec 范围。
