<script setup lang="ts">
import { ref, computed, onMounted } from 'vue'
import { useAgentsStore } from '@/stores/agents'
import { usePersonalities, type PersonaDiffPreview } from '@/composables/usePersonalities'
import { showToast } from '@/composables/useToast'
import { invoke } from '@tauri-apps/api/core'
import { lineDiff, type DiffLine } from '@/utils/diff'
import type { Persona, PersonaFileContent } from '@/types'

const agentsStore = useAgentsStore()
const toast = showToast
const {
  listPersonalities,
  savePersonality,
  switchPersonality,
  deletePersonality,
  previewImport,
  importPersonalities
} = usePersonalities()

// 按 agentId 分组的 personas
const grouped = ref<Record<string, Persona[]>>({})
const collapsedAgents = ref<Set<string>>(new Set())

const selected = ref<{ agentId: string; name: string } | null>(null)
const selectedPersona = ref<Persona | null>(null)
const selFile = ref<string | null>(null)
const selMode = ref<'render' | 'source' | 'diff'>('render')

const fileContent = ref<PersonaFileContent | null>(null)
const loadingFile = ref(false)

const collapsedFiles = ref<Set<string>>(new Set())

const totalPersonas = computed(() =>
  Object.values(grouped.value).reduce((s, arr) => s + arr.length, 0)
)

// ---------- 树 / 预览辅助 ----------
interface TreeNode {
  name: string
  path: string
  children: TreeNode[]
  isDir: boolean
}

function esc(s: string): string {
  return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
}

function fileGlyph(file: string): string {
  let icon = '📄'
  if (/\.md$/i.test(file)) icon = '📜'
  else if (/\.json$/i.test(file)) icon = '⚙️'
  else if (/\.(ts|js|vue|py|rs|toml|ya?ml|css)$/i.test(file)) icon = '⟨⟩'
  return `<span style="font-size:13px;width:14px;text-align:center">${icon}</span>`
}

function buildTree(files: string[]): TreeNode[] {
  const root: TreeNode[] = []
  const dirMap = new Map<string, TreeNode>()
  for (const f of files) {
    const parts = f.split('/')
    let cur = root
    let prefix = ''
    for (let i = 0; i < parts.length; i++) {
      const part = parts[i]
      prefix = prefix ? `${prefix}/${part}` : part
      const isLast = i === parts.length - 1
      if (isLast) {
        cur.push({ name: part, path: prefix, children: [], isDir: false })
      } else {
        let node = dirMap.get(prefix)
        if (!node) {
          node = { name: part, path: prefix, children: [], isDir: true }
          dirMap.set(prefix, node)
          cur.push(node)
        }
        cur = node.children
      }
    }
  }
  return root
}

function renderTree(nodes: TreeNode[], depth: number): string {
  let html = ''
  for (const n of nodes) {
    if (n.isDir) {
      const isCollapsed = collapsedFiles.value.has(n.path)
      html += `<div class="tnode${isCollapsed ? ' is-collapsed' : ''}">
        <div class="trow trow--dir" data-dir="${esc(n.path)}" style="padding-left:${8 + depth * 14}px">
          <span class="tcaret"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M9 6l6 6-6 6"/></svg></span>
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6"><path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"/></svg>
          <span>${esc(n.name)}</span>
        </div>
        <div class="tchildren">${renderTree(n.children, depth + 1)}</div>
      </div>`
    } else {
      const active = n.path === selFile.value ? ' is-active' : ''
      html += `<div class="trow trow--file${active}" data-file="${esc(n.path)}" style="padding-left:${8 + depth * 14 + 18}px">
        ${fileGlyph(n.path)}<span>${esc(n.name)}</span>
      </div>`
    }
  }
  return html
}

const fileTreeHtml = computed(() => {
  if (!selectedPersona.value) return ''
  return renderTree(buildTree(selectedPersona.value.files), 0)
})

function highlight(file: string, line: string): string {
  const e = esc(line)
  if (/\.json$/i.test(file)) {
    return e
      .replace(/(&quot;[^&]*?&quot;)(\s*:)/g, '<span class="kw">$1</span>$2')
      .replace(/:\s*(&quot;[^&]*?&quot;|[\d.]+|true|false|null)/g, ': <span class="mut">$1</span>')
  }
  return e
}

/** 行内 markdown 格式：code 优先（避免内部 * 被误吞），再 bold，再 italic */
function inline(s: string): string {
  let e = esc(s)
  e = e.replace(/`([^`]+)`/g, '<code>$1</code>')
  e = e.replace(/\*\*([^*]+)\*\*/g, '<strong>$1</strong>')
  e = e.replace(/(^|[^*])\*([^*]+)\*/g, '$1<em>$2</em>')
  e = e.replace(/_([^_]+)_/g, '<em>$1</em>')
  return e
}

function renderDoc(lines: string[]): string {
  let html = ''
  let inList = false
  const closeList = () => {
    if (inList) {
      html += '</ul>'
      inList = false
    }
  }
  for (const raw of lines) {
    if (/^## /.test(raw)) {
      closeList()
      html += `<h2>${inline(raw.slice(3))}</h2>`
    } else if (/^# /.test(raw)) {
      closeList()
      html += `<h1>${inline(raw.slice(2))}</h1>`
    } else if (/^> /.test(raw)) {
      closeList()
      html += `<blockquote>${inline(raw.slice(2))}</blockquote>`
    } else if (/^- /.test(raw)) {
      if (!inList) {
        html += '<ul>'
        inList = true
      }
      html += `<li>${inline(raw.slice(2))}</li>`
    } else if (raw.trim() === '') {
      closeList()
    } else {
      closeList()
      html += `<p>${inline(raw)}</p>`
    }
  }
  closeList()
  return html
}

/** source 模式纯文本渲染（带行号 + 高亮），供 renderPane 和 diff 边界场景复用 */
function renderSourcePane(content: string, file: string): string {
  if (!content) {
    return '<div class="empty"><div class="empty__icon">📄</div><div class="empty__title">文件内容为空</div></div>'
  }
  const lines = content.split('\n')
  return `<div class="code">${lines
    .map((l, i) => `<div><span class="ln">${i + 1}</span>${highlight(file, l)}</div>`)
    .join('')}</div>`
}

/** 渲染单侧内容（render 或 source 模式） */
function renderPane(content: string | null, file: string, mode: 'render' | 'source'): string {
  if (content == null) {
    return '<div class="empty"><div class="empty__icon">📭</div><div class="empty__title">无此文件</div></div>'
  }
  if (mode === 'render') {
    if (/\.md$/i.test(file)) {
      return `<div class="doc">${renderDoc(content.split('\n'))}</div>`
    }
    return '<div class="empty"><div class="empty__icon">📄</div><div class="empty__title">无渲染视图</div><div>该文件类型暂不支持渲染，请切换到「源码」</div></div>'
  }
  return renderSourcePane(content, file)
}

/** diff 双栏：把扁平 DiffLine[] 拆成左右对齐行。left 显示 same+del，right 显示 same+add */
function renderDiffPane(side: 'left' | 'right'): string {
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
    // 本地新增文件：左栏提示，右栏显示本地全部
    return side === 'left'
      ? '<div class="diff__h">本地新增文件</div>'
      : renderSourcePane(local, selFile.value || '')
  }
  if (!fc.localContent) {
    // 本地无此文件（人格独有）：右栏提示，左栏显示人格全部
    return side === 'right'
      ? '<div class="diff__h">本地无此文件（人格独有）</div>'
      : renderSourcePane(persona, selFile.value || '')
  }
  const diff: DiffLine[] = lineDiff(persona, local)
  let html = `<div class="diff__h">@@ 行差异 · ${diff.length} 行 @@</div>`
  for (const d of diff) {
    if (d.type === 'same') {
      html += `<div class="diff__row"><span class="diff__sign"> </span><span class="diff__txt">${esc(d.text)}</span></div>`
    } else if (d.type === 'del') {
      html += side === 'left'
        ? `<div class="diff__row diff__del"><span class="diff__sign">-</span><span class="diff__txt">${esc(d.text)}</span></div>`
        : `<div class="diff__row diff__placeholder"></div>`
    } else {
      html += side === 'right'
        ? `<div class="diff__row diff__add"><span class="diff__sign">+</span><span class="diff__txt">${esc(d.text)}</span></div>`
        : `<div class="diff__row diff__placeholder"></div>`
    }
  }
  return html
}

/** 前置空态：loading / 未选 / 二进制 -- 两栏共用 */
function paneEmptyState(): string | null {
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
  return null
}

const leftPaneHtml = computed(() => {
  const empty = paneEmptyState()
  if (empty) return empty
  const fc = fileContent.value!
  const file = selFile.value!
  if (selMode.value === 'diff') return renderDiffPane('left')
  return renderPane(fc.personaContent, file, selMode.value === 'source' ? 'source' : 'render')
})

const rightPaneHtml = computed(() => {
  const empty = paneEmptyState()
  if (empty) return empty
  const fc = fileContent.value!
  const file = selFile.value!
  if (selMode.value === 'diff') return renderDiffPane('right')
  return renderPane(fc.localContent, file, selMode.value === 'source' ? 'source' : 'render')
})

// ---------- 交互 ----------
function toggleAgent(id: string) {
  const s = new Set(collapsedAgents.value)
  if (s.has(id)) s.delete(id)
  else s.add(id)
  collapsedAgents.value = s
}

function selectPersona(agentId: string, name: string) {
  selected.value = { agentId, name }
  const p = (grouped.value[agentId] || []).find((x) => x.name === name) || null
  selectedPersona.value = p
  selFile.value = p && p.files.length ? p.files[0] : null
  loadFileContent()
}

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

function onFileTreeClick(e: MouseEvent) {
  const target = e.target as HTMLElement
  const dirEl = target.closest('.trow--dir')
  if (dirEl) {
    const path = (dirEl as HTMLElement).dataset.dir
    if (path) {
      const s = new Set(collapsedFiles.value)
      if (s.has(path)) s.delete(path)
      else s.add(path)
      collapsedFiles.value = s
    }
    return
  }
  const fileEl = target.closest('.trow--file')
  if (fileEl) {
    const f = (fileEl as HTMLElement).dataset.file
    if (f) selectFile(f)
  }
}

async function onSwitch(name: string) {
  if (!selected.value) return
  try {
    await switchPersonality(selected.value.agentId, name)
    toast(`已切换到 ${name}`)
    await reloadAgent(selected.value.agentId)
  } catch (e) {
    toast(`切换失败: ${e}`)
  }
}

async function onDelete(name: string) {
  if (!selected.value) return
  try {
    await deletePersonality(selected.value.agentId, name)
    toast(`已删除 ${name}`)
    await reloadAgent(selected.value.agentId)
    if (selected.value.name === name) {
      selected.value = null
      selectedPersona.value = null
    }
  } catch (e) {
    toast(`删除失败: ${e}`)
  }
}

async function reloadAgent(agentId: string) {
  try {
    grouped.value[agentId] = await listPersonalities(agentId)
  } catch {
    grouped.value[agentId] = []
  }
}

// 保存弹窗
const saveDialogShow = ref(false)
const saveName = ref('')
function openSaveDialog() {
  saveName.value = ''
  saveDialogShow.value = true
}
async function confirmSave() {
  if (!selected.value) return
  if (!saveName.value.trim()) {
    toast('请输入人格名称')
    return
  }
  try {
    await savePersonality(selected.value.agentId, saveName.value.trim())
    toast(`已保存人格 ${saveName.value}`)
    saveDialogShow.value = false
    await reloadAgent(selected.value.agentId)
  } catch (e) {
    toast(`保存失败: ${e}`)
  }
}

// 导入
const importDialogShow = ref(false)
const importPreviews = ref<PersonaDiffPreview[]>([])
const importZipPath = ref('')
const importConfirmed = ref(false)
const importing = ref(false)

async function onImportClick() {
  if (!selected.value) {
    toast('请先选择 Agent')
    return
  }
  try {
    const result = await previewImport(selected.value.agentId)
    if (!result) return
    importPreviews.value = result.previews
    importZipPath.value = result.zipPath
    importConfirmed.value = false
    importDialogShow.value = true
  } catch (e) {
    toast(`导入预览失败: ${e}`)
  }
}

async function confirmImport() {
  if (!selected.value) return
  importing.value = true
  try {
    await importPersonalities(importZipPath.value, selected.value.agentId)
    toast('导入成功')
    importDialogShow.value = false
    await reloadAgent(selected.value.agentId)
  } catch (e) {
    toast(`导入失败: ${e}`)
  } finally {
    importing.value = false
  }
}

const actionText: Record<string, string> = {
  added: '新增',
  modified: '修改',
  unchanged: '未变更'
}

onMounted(async () => {
  try {
    await agentsStore.loadAgents()
    for (const a of agentsStore.agents) {
      try {
        grouped.value[a.id] = await listPersonalities(a.id)
      } catch {
        grouped.value[a.id] = []
      }
    }
    const first = agentsStore.agents[0]
    if (first && (grouped.value[first.id] || []).length) {
      selectPersona(first.id, grouped.value[first.id][0].name)
    }
  } catch (e) {
    toast(`加载失败: ${e}`)
  }
})
</script>

<template>
  <div class="pl">
    <!-- 左：树形人格列表 -->
    <aside class="pl__master">
      <div class="agent-rail__head">
        <span class="agent-rail__title">人格</span>
        <div style="display: flex; align-items: center; gap: 8px">
          <span class="agent-rail__count">{{ totalPersonas }}</span>
          <button class="btn btn--quiet" style="padding: 4px 8px; font-size: 12px" @click="onImportClick">导入</button>
        </div>
      </div>
      <div class="pl__master-list">
        <div
          v-for="agent in agentsStore.agents"
          :key="agent.id"
          class="tnode"
          :class="{ 'is-collapsed': collapsedAgents.has(agent.id) }"
        >
          <div
            class="trow trow--dir"
            role="button"
            tabindex="0"
            :aria-expanded="!collapsedAgents.has(agent.id)"
            @click="toggleAgent(agent.id)"
            @keydown.enter.prevent="toggleAgent(agent.id)"
          >
            <span class="tcaret"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M9 6l6 6-6 6" /></svg></span>
            <span class="agent__glyph" style="width:18px;height:18px;font-size:10px;border-radius:5px" :style="{ background: agent.accentColor || '#5B4FE9' }">{{ (agent.displayName || agent.id).charAt(0).toUpperCase() }}</span>
            <span>{{ agent.displayName }}</span>
            <span class="agent-rail__count" style="margin-left:auto">{{ (grouped[agent.id] || []).length }}</span>
          </div>
          <div class="tchildren">
            <div
              v-for="p in (grouped[agent.id] || [])"
              :key="p.name"
              class="trow trow--file"
              :class="{ 'is-active': selected?.agentId === agent.id && selected?.name === p.name }"
              role="button"
              tabindex="0"
              @click="selectPersona(agent.id, p.name)"
              @keydown.enter.prevent="selectPersona(agent.id, p.name)"
            >
              <span style="width:14px" />
              <span>{{ p.displayName }}</span>
              <span v-if="p.isCurrent" class="agent__cur" style="margin-left:auto">当前</span>
            </div>
          </div>
        </div>
        <div v-if="totalPersonas === 0" class="empty">
          <div class="empty__icon">🧩</div>
          <div class="empty__title">暂无人格</div>
        </div>
      </div>
    </aside>

    <!-- 右：详情 + 文件树 + 预览 -->
    <section class="pl__detail">
      <div v-if="!selectedPersona" class="empty">
        <div class="empty__icon">📄</div>
        <div class="empty__title">选择左侧人格查看文件</div>
      </div>

      <template v-else>
        <div class="pl__detail-head">
          <div>
            <h3 class="pl__detail-title">{{ selectedPersona.displayName }}</h3>
            <div class="pl__detail-sub">
              {{ selected?.agentId }} · {{ selectedPersona.files.length }} 文件 · {{ Math.round(selectedPersona.sizeBytes / 1024) }} KB
            </div>
          </div>
          <div class="pl__detail-acts">
            <button class="btn btn--primary" @click="onSwitch(selectedPersona.name)">切换</button>
            <button class="btn btn--danger" @click="onDelete(selectedPersona.name)">删除</button>
            <button class="btn btn--ghost" @click="openSaveDialog">保存当前</button>
          </div>
        </div>

        <div class="pl__body">
          <div class="file-rail">
            <div class="file-rail__head">
              <span>文件</span>
              <span>{{ selectedPersona.files.length }}</span>
            </div>
            <div class="file-rail__list" @click="onFileTreeClick" v-html="fileTreeHtml" />
          </div>

          <div class="preview">
            <div class="preview__bar">
              <div class="preview__tabs">
                <button :class="{ 'is-on': selMode === 'render' }" @click="selMode = 'render'">渲染</button>
                <button :class="{ 'is-on': selMode === 'source' }" @click="selMode = 'source'">源码</button>
                <button :class="{ 'is-on': selMode === 'diff' }" @click="selMode = 'diff'">差异</button>
              </div>
              <span class="preview__hint">预览 · 只读</span>
            </div>
            <div class="preview__split">
              <div class="preview__pane">
                <div class="preview__pane-head">人格快照</div>
                <div class="preview__pane-body" v-html="leftPaneHtml" />
              </div>
              <div class="preview__pane">
                <div class="preview__pane-head">当前 · 本地</div>
                <div class="preview__pane-body" v-html="rightPaneHtml" />
              </div>
            </div>
          </div>
        </div>
      </template>
    </section>
  </div>

  <!-- 保存弹窗 -->
  <div v-if="saveDialogShow" class="modal-mask" @click.self="saveDialogShow = false">
    <div class="modal">
      <div class="modal__head"><h3 class="modal__title">保存当前为人格</h3></div>
      <div class="modal__body">
        <div class="field">
          <label class="field__label">人格名称</label>
          <input v-model="saveName" class="inp" placeholder="如 work-mode" @keydown.enter="confirmSave" />
        </div>
      </div>
      <div class="modal__foot">
        <button class="btn btn--ghost" @click="saveDialogShow = false">取消</button>
        <button class="btn btn--primary" @click="confirmSave">保存</button>
      </div>
    </div>
  </div>

  <!-- 导入弹窗 -->
  <div v-if="importDialogShow" class="modal-mask" @click.self="importDialogShow = false">
    <div class="modal" style="width: 680px">
      <div class="modal__head"><h3 class="modal__title">导入人格包 · Diff 预览</h3></div>
      <div class="modal__body">
        <div class="notice notice--warn">
          导入的人格文件本质上是 AI 助手的系统提示词，恶意人格包可能包含 prompt 注入攻击。请仔细审查以下变更。
        </div>
        <div v-for="pv in importPreviews" :key="pv.name" style="margin-top: 14px">
          <div style="font-weight: 600; margin-bottom: 6px">{{ pv.displayName }}</div>
          <div v-for="f in pv.files" :key="f.path" class="conflict-file" style="display: flex; align-items: center; justify-content: space-between">
            <span>{{ f.path }}</span>
            <span class="tag">{{ actionText[f.action] }}</span>
          </div>
        </div>
        <div class="strat" :class="{ 'is-on': importConfirmed }" style="margin-top: 16px; cursor: pointer" @click="importConfirmed = !importConfirmed">
          <div class="strat__t">我已审查导入内容并理解风险</div>
        </div>
      </div>
      <div class="modal__foot">
        <button class="btn btn--ghost" @click="importDialogShow = false">取消</button>
        <button class="btn btn--primary" :disabled="!importConfirmed || importing" @click="confirmImport">
          {{ importing ? '导入中…' : '确认导入' }}
        </button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.pl__master-list { flex: 1; min-height: 0; overflow-y: auto; padding: var(--s-2); }
</style>
