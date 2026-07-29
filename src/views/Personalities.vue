<script setup lang="ts">
import { ref, onMounted } from 'vue'
import {
  NEmpty,
  NButton,
  NSpace,
  NTag,
  NInput,
  NModal,
  NList,
  NListItem,
  NThing,
  NCheckbox,
  NAlert,
  NSpin,
  useMessage
} from 'naive-ui'
import { useAgentsStore } from '@/stores/agents'
import { usePersonalities, type PersonaDiffPreview } from '@/composables/usePersonalities'
import type { Persona } from '@/types'

const message = useMessage()
const agentsStore = useAgentsStore()
const {
  listPersonalities,
  savePersonality,
  switchPersonality,
  deletePersonality,
  exportPersonalities,
  previewImport,
  importPersonalities
} = usePersonalities()

const personalities = ref<Persona[]>([])
const selectedAgentId = ref<string>('')
const selectedPersona = ref<Persona | null>(null)
const loading = ref(false)

// 保存人格弹窗
const saveDialogShow = ref(false)
const saveName = ref('')

// 导入预览弹窗
const importDialogShow = ref(false)
const importPreviews = ref<PersonaDiffPreview[]>([])
const importZipPath = ref('')
const importConfirmed = ref(false)
const importing = ref(false)

onMounted(async () => {
  try {
    await agentsStore.loadAgents()
    if (agentsStore.agents.length > 0) {
      selectedAgentId.value = agentsStore.agents[0].id
      await loadPersonalities()
    }
  } catch (e) {
    message.warning(`加载 agents 失败: ${e}`)
  }
})

async function loadPersonalities() {
  if (!selectedAgentId.value) return
  loading.value = true
  try {
    personalities.value = await listPersonalities(selectedAgentId.value)
    // 默认选第一个
    if (personalities.value.length > 0 && !selectedPersona.value) {
      selectedPersona.value = personalities.value[0]
    }
  } catch (e) {
    message.error(`加载人格失败: ${e}`)
  } finally {
    loading.value = false
  }
}

function onSelectPersona(p: Persona) {
  selectedPersona.value = p
}

async function onSwitch(name: string) {
  try {
    await switchPersonality(selectedAgentId.value, name)
    message.success(`已切换到 ${name}`)
    await loadPersonalities()
  } catch (e) {
    message.error(`切换失败: ${e}`)
  }
}

async function onDelete(name: string) {
  try {
    await deletePersonality(selectedAgentId.value, name)
    message.success(`已删除 ${name}`)
    await loadPersonalities()
  } catch (e) {
    message.error(`删除失败: ${e}`)
  }
}

function openSaveDialog() {
  saveName.value = ''
  saveDialogShow.value = true
}

async function confirmSave() {
  if (!saveName.value.trim()) {
    message.warning('请输入人格名称')
    return
  }
  try {
    await savePersonality(selectedAgentId.value, saveName.value.trim())
    message.success(`已保存人格 ${saveName.value}`)
    saveDialogShow.value = false
    await loadPersonalities()
  } catch (e) {
    message.error(`保存失败: ${e}`)
  }
}

async function onExport() {
  const selected = personalities.value.map((p) => p.name)
  if (selected.length === 0) {
    message.warning('没有可导出的人格')
    return
  }
  try {
    const path = await exportPersonalities(selectedAgentId.value, selected)
    if (path) {
      message.success(`已导出到 ${path}`)
    }
  } catch (e) {
    message.error(`导出失败: ${e}`)
  }
}

async function onImportClick() {
  try {
    const result = await previewImport(selectedAgentId.value)
    if (!result) return
    importPreviews.value = result.previews
    importZipPath.value = result.zipPath
    importConfirmed.value = false
    importDialogShow.value = true
  } catch (e) {
    message.error(`导入预览失败: ${e}`)
  }
}

async function confirmImport() {
  importing.value = true
  try {
    await importPersonalities(importZipPath.value, selectedAgentId.value)
    message.success('导入成功')
    importDialogShow.value = false
    await loadPersonalities()
  } catch (e) {
    message.error(`导入失败: ${e}`)
  } finally {
    importing.value = false
  }
}

const actionText: Record<string, string> = {
  added: '新增',
  modified: '修改',
  unchanged: '未变更'
}
const actionType: Record<string, 'success' | 'warning' | 'default'> = {
  added: 'success',
  modified: 'warning',
  unchanged: 'default'
}
</script>

<template>
  <div class="personalities">
    <!-- 左侧：人格列表（深色） -->
    <aside class="personalities__sidebar">
      <div class="personalities__sidebar-header">
        <span class="personalities__sidebar-title">人格列表</span>
        <select
          v-if="agentsStore.agents.length > 0"
          v-model="selectedAgentId"
          class="personalities__agent-select"
          @change="loadPersonalities"
        >
          <option v-for="a in agentsStore.agents" :key="a.id" :value="a.id">
            {{ a.displayName }}
          </option>
        </select>
      </div>

      <div class="personalities__list">
        <n-spin v-if="loading" size="small" />
        <n-empty
          v-else-if="personalities.length === 0"
          size="small"
          description="暂无人格"
        />
        <div
          v-for="p in personalities"
          :key="p.name"
          class="personalities__item"
          :class="{ 'personalities__item--active': selectedPersona?.name === p.name }"
          @click="onSelectPersona(p)"
        >
          <div class="personalities__item-header">
            <span class="personalities__item-name">{{ p.displayName }}</span>
            <n-tag v-if="p.importedAt" type="warning" size="small">导入</n-tag>
          </div>
          <div class="personalities__item-meta">
            <span>{{ p.files.length }} 文件</span>
            <span>{{ Math.round(p.sizeBytes / 1024) }} KB</span>
          </div>
          <div class="personalities__item-actions">
            <n-button size="tiny" @click="onSwitch(p.name)">切换</n-button>
            <n-button size="tiny" type="error" ghost @click="onDelete(p.name)">删除</n-button>
          </div>
        </div>
      </div>

      <div class="personalities__sidebar-footer">
        <n-space>
          <n-button size="small" @click="openSaveDialog">保存当前</n-button>
          <n-button size="small" @click="onImportClick">导入人格包</n-button>
          <n-button size="small" @click="onExport">导出全部</n-button>
        </n-space>
      </div>
    </aside>

    <!-- 右侧：文件预览面板 -->
    <section class="personalities__main">
      <div v-if="personalities.length === 0" class="personalities__empty">
        <n-empty description="请先保存一个人格，或从左侧选择查看文件内容" />
      </div>
      <template v-else>
        <div class="personalities__file-tabs">
          <div class="personalities__file-tabs-title">
            {{ selectedPersona ? selectedPersona.displayName + ' 的文件' : '请从左侧选择人格' }}
          </div>
        </div>
        <div class="personalities__editor">
          <div v-if="!selectedPersona" class="personalities__editor-placeholder">
            选中人格后可查看文件列表
          </div>
          <div v-else class="personalities__file-list">
            <div
              v-for="file in selectedPersona.files"
              :key="file"
              class="personalities__file-item"
            >
              <span class="personalities__file-icon">📄</span>
              <span class="personalities__file-name">{{ file }}</span>
            </div>
            <div v-if="selectedPersona.files.length === 0" class="personalities__editor-placeholder">
              该人格没有文件
            </div>
          </div>
        </div>
        <div class="personalities__statusbar">
          <span>UTF-8</span>
          <span>{{ selectedPersona ? selectedPersona.files.length : 0 }} 个文件</span>
        </div>
      </template>
    </section>

    <!-- 保存人格弹窗 -->
    <n-modal
      v-model:show="saveDialogShow"
      preset="card"
      title="保存当前为人格"
      style="width: 400px"
    >
      <n-input v-model:value="saveName" placeholder="人格名称，如 work-mode" />
      <template #footer>
        <n-space justify="end">
          <n-button @click="saveDialogShow = false">取消</n-button>
          <n-button type="primary" @click="confirmSave">保存</n-button>
        </n-space>
      </template>
    </n-modal>

    <!-- 导入预览弹窗 -->
    <n-modal
      v-model:show="importDialogShow"
      preset="card"
      title="导入人格包 - Diff 预览"
      style="width: 700px; max-width: 90vw"
    >
      <div class="import-dialog">
        <n-alert type="warning" :bordered="false">
          导入的人格文件本质上是 AI 助手的系统提示词，恶意人格包可能包含 prompt 注入攻击。请仔细审查以下变更。
        </n-alert>

        <div v-for="preview in importPreviews" :key="preview.name" class="import-dialog__persona">
          <div class="import-dialog__persona-name">{{ preview.displayName }}</div>
          <n-list bordered size="small">
            <n-list-item v-for="file in preview.files" :key="file.path">
              <n-thing>
                <template #header>
                  <span class="import-dialog__file-path">{{ file.path }}</span>
                </template>
                <template #description>
                  <n-tag :type="actionType[file.action]" size="small">
                    {{ actionText[file.action] }}
                  </n-tag>
                </template>
              </n-thing>
            </n-list-item>
          </n-list>
        </div>

        <n-checkbox v-model:checked="importConfirmed" class="import-dialog__confirm">
          我已审查导入内容并理解风险
        </n-checkbox>
      </div>

      <template #footer>
        <n-space justify="end">
          <n-button @click="importDialogShow = false">取消</n-button>
          <n-button
            type="primary"
            :disabled="!importConfirmed"
            :loading="importing"
            @click="confirmImport"
          >
            确认导入
          </n-button>
        </n-space>
      </template>
    </n-modal>
  </div>
</template>

<style scoped>
.personalities {
  display: flex;
  height: 100%;
}

/* 左侧：深色 */
.personalities__sidebar {
  width: 280px;
  flex-shrink: 0;
  background: #1e1e1e;
  color: #e4e4e7;
  display: flex;
  flex-direction: column;
  border-right: 1px solid #27272a;
  overflow: hidden;
}
.personalities__sidebar-header {
  padding: 16px;
  border-bottom: 1px solid #27272a;
}
.personalities__sidebar-title {
  font-size: 13px;
  font-weight: 600;
  color: #a1a1aa;
  text-transform: uppercase;
  letter-spacing: 0.5px;
  display: block;
  margin-bottom: 8px;
}
.personalities__agent-select {
  width: 100%;
  background: #27272a;
  color: #e4e4e7;
  border: 1px solid #3f3f46;
  border-radius: 4px;
  padding: 4px 8px;
  font-size: 13px;
}
.personalities__list {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  padding: 8px;
}
.personalities__item {
  padding: 10px 12px;
  border-radius: 6px;
  margin-bottom: 4px;
  background: #27272a;
  cursor: pointer;
  transition: all 0.15s;
}
.personalities__item:hover {
  background: #3f3f46;
}
.personalities__item--active {
  background: #3f3f46;
  border-left: 3px solid #3b82f6;
}
.personalities__item-header {
  display: flex;
  align-items: center;
  gap: 6px;
  margin-bottom: 6px;
}
.personalities__item-name {
  flex: 1;
  font-size: 13px;
  color: #e4e4e7;
}
.personalities__item-meta {
  display: flex;
  gap: 12px;
  font-size: 11px;
  color: #71717a;
  margin-bottom: 8px;
}
.personalities__item-actions {
  display: flex;
  gap: 4px;
}
.personalities__sidebar-footer {
  flex-shrink: 0;
  padding: 12px;
  border-top: 1px solid #27272a;
}

/* 深色侧边栏内按钮强制浅色文字 */
.personalities__sidebar :deep(.n-button) {
  color: #e4e4e7 !important;
}
.personalities__sidebar :deep(.n-button:hover) {
  color: #fff !important;
}
.personalities__sidebar :deep(.n-button--error-type) {
  color: #f87171 !important;
}

/* 右侧：浅色 + 暗色编辑器 */
.personalities__main {
  flex: 1;
  display: flex;
  flex-direction: column;
  overflow: hidden;
  background: #fff;
}
.personalities__empty {
  flex: 1;
  display: flex;
  align-items: center;
  justify-content: center;
}
.personalities__file-tabs {
  padding: 8px 16px 0;
  border-bottom: 1px solid #e4e4e7;
}
.personalities__file-tabs-title {
  padding: 8px 0;
  font-size: 12px;
  color: #a1a1aa;
}
.personalities__editor {
  flex: 1;
  background: #1e293b;
  overflow: auto;
}
.personalities__editor-placeholder {
  padding: 16px;
  color: #64748b;
  font-size: 13px;
  font-family: 'Consolas', 'Monaco', monospace;
}
.personalities__file-list {
  padding: 8px 0;
}
.personalities__file-item {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 6px 16px;
  color: #cbd5e1;
  font-size: 13px;
  font-family: 'Consolas', 'Monaco', monospace;
}
.personalities__file-item:hover {
  background: #334155;
}
.personalities__file-icon {
  font-size: 14px;
}
.personalities__file-name {
  flex: 1;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.personalities__statusbar {
  display: flex;
  gap: 16px;
  padding: 6px 16px;
  background: #f4f4f5;
  border-top: 1px solid #e4e4e7;
  font-size: 11px;
  color: #71717a;
}

/* 导入弹窗 */
.import-dialog {
  display: flex;
  flex-direction: column;
  gap: 16px;
  max-height: 50vh;
  overflow-y: auto;
}
.import-dialog__persona {
  margin-top: 8px;
}
.import-dialog__persona-name {
  font-weight: 600;
  margin-bottom: 8px;
}
.import-dialog__file-path {
  font-family: 'Consolas', 'Monaco', monospace;
  font-size: 13px;
}
.import-dialog__confirm {
  margin-top: 8px;
}
</style>
