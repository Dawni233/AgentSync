<script setup lang="ts">
import { ref, onMounted, onUnmounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import {
  NButton,
  NSpace,
  NEmpty,
  NDataTable,
  NModal,
  NInput,
  NList,
  NListItem,
  NThing,
  useMessage
} from 'naive-ui'
import type { DataTableColumns } from 'naive-ui'
import { useAgentsStore } from '@/stores/agents'
import { useSyncStore } from '@/stores/sync'
import { useSyncEvents } from '@/composables/useSync'
import { usePersonalities } from '@/composables/usePersonalities'
import SyncStatusBadge from '@/components/SyncStatusBadge.vue'
import ConflictDialog from '@/components/ConflictDialog.vue'
import type { Agent, ConflictDetectedPayload, Persona } from '@/types'

const agentsStore = useAgentsStore()
const syncStore = useSyncStore()
const message = useMessage()

const activeAgent = ref<Agent | null>(null)

// 开发自检
const devPingResult = ref('')
const devAppDataDir = ref('')
async function devPing() {
  devPingResult.value = await invoke<string>('ping', { name: 'Dashboard' })
  devAppDataDir.value = await invoke<string>('get_app_data_dir')
}

// 冲突弹窗状态
const conflictShow = ref(false)
const conflictPayload = ref<ConflictDetectedPayload | null>(null)
const conflictResolving = ref(false)
let unlistenEvents: (() => void) | null = null

onMounted(async () => {
  // 加载 agent 列表
  try {
    await agentsStore.loadAgents()
  } catch (e) {
    message.warning(`加载 agents 失败: ${e}`)
  }

  // 监听同步事件
  unlistenEvents = await useSyncEvents({
    onConflict: (payload) => {
      conflictPayload.value = payload
      conflictShow.value = true
    }
  })
})

onUnmounted(() => {
  if (unlistenEvents) unlistenEvents()
})

async function onSelectAgent(agent: Agent) {
  activeAgent.value = agent
  await loadTrackedFiles(agent.id)
  await loadAgentPersonalities(agent.id)
}

// 人格切换/保存
const { listPersonalities, savePersonality, switchPersonality } = usePersonalities()
const agentPersonalities = ref<Persona[]>([])
const showSwitchDialog = ref(false)
const showSaveDialog = ref(false)
const savePersonaName = ref('')
const switching = ref(false)
const saving = ref(false)

async function loadAgentPersonalities(agentId: string) {
  try {
    agentPersonalities.value = await listPersonalities(agentId)
  } catch (e) {
    agentPersonalities.value = []
  }
}

async function onSwitchPersona(name: string) {
  if (!activeAgent.value) return
  switching.value = true
  try {
    await switchPersonality(activeAgent.value.id, name)
    message.success(`已切换到 ${name}`)
    showSwitchDialog.value = false
    await agentsStore.loadAgents()
    // 更新当前选中 agent
    const updated = agentsStore.agents.find((a) => a.id === activeAgent.value?.id)
    if (updated) activeAgent.value = updated
  } catch (e) {
    message.error(`切换失败: ${e}`)
  } finally {
    switching.value = false
  }
}

async function onSavePersona() {
  if (!activeAgent.value) return
  if (!savePersonaName.value.trim()) {
    message.warning('请输入人格名称')
    return
  }
  saving.value = true
  try {
    await savePersonality(activeAgent.value.id, savePersonaName.value.trim())
    message.success(`已保存人格 ${savePersonaName.value}`)
    showSaveDialog.value = false
    savePersonaName.value = ''
    await loadAgentPersonalities(activeAgent.value.id)
  } catch (e) {
    message.error(`保存失败: ${e}`)
  } finally {
    saving.value = false
  }
}

async function onConflictResolve(option: 'L1ExportPatch' | 'L1DiscardLocal' | 'L1Cancel' | 'L2KeepLocal' | 'L2KeepRemote' | 'L2ManualMerge' | 'L2Cancel') {
  conflictResolving.value = true
  try {
    await syncStore.resolveConflict({ type: option })
    conflictShow.value = false
    conflictPayload.value = null
    // 刷新 agent 列表
    await agentsStore.loadAgents()
  } catch (e) {
    message.error(`解决冲突失败: ${e}`)
  } finally {
    conflictResolving.value = false
  }
}

function onConflictCancel() {
  // 取消同步 = L1Cancel 或 L2Cancel
  const option = conflictPayload.value?.conflictType === 'L1' ? 'L1Cancel' : 'L2Cancel'
  onConflictResolve(option)
}

// 文件列表列定义
interface TrackedFile {
  name: string
  size: string
  mtime: string
}
const fileColumns: DataTableColumns<TrackedFile> = [
  { title: '文件名', key: 'name' },
  { title: '大小', key: 'size', width: 100 },
  { title: '最近变更', key: 'mtime', width: 180 }
]
const trackedFiles = ref<TrackedFile[]>([])
const loadingFiles = ref(false)

// 选中 agent 时加载跟踪文件
async function loadTrackedFiles(agentId: string) {
  loadingFiles.value = true
  try {
    const files = await invoke<Array<{ name: string; sizeBytes: number; modifiedAt: number | null }>>(
      'list_tracked_files',
      { agentId }
    )
    trackedFiles.value = files.map((f) => ({
      name: f.name,
      size: f.sizeBytes < 1024 ? `${f.sizeBytes} B` : `${(f.sizeBytes / 1024).toFixed(1)} KB`,
      mtime: f.modifiedAt ? new Date(f.modifiedAt).toLocaleString() : '-'
    }))
  } catch (e) {
    message.error(`加载文件列表失败: ${e}`)
    trackedFiles.value = []
  } finally {
    loadingFiles.value = false
  }
}

// Vue h 函数（用于 DataTable render）

</script>

<template>
  <div class="dashboard">
    <aside class="dashboard__sidebar">
      <div class="dashboard__sidebar-header">
        <span class="dashboard__sidebar-title" @click="devPing">Agents</span>
        <span class="dashboard__sidebar-count">{{ agentsStore.agents.length }}</span>
      </div>
      <div v-if="devPingResult" class="dashboard__dev">IPC: <span class="dashboard__dev-ok">✓</span></div>
      <div class="dashboard__agent-list">
        <div v-if="agentsStore.agents.length === 0" class="dashboard__agent-empty">
          <div class="dashboard__agent-empty-icon">🤖</div>
          <p>暂无 Agent</p>
          <n-button text size="small" @click="agentsStore.loadAgents">刷新</n-button>
        </div>
        <div v-for="agent in agentsStore.agents" :key="agent.id"
          class="dashboard__agent-item"
          :class="{ 'dashboard__agent-item--active': activeAgent?.id === agent.id }"
          @click="onSelectAgent(agent)">
          <div class="dashboard__agent-info">
            <span class="dashboard__agent-name">{{ agent.displayName }}</span>
            <span class="dashboard__agent-persona">{{ agent.currentPersona || '默认' }}</span>
          </div>
          <SyncStatusBadge :status="agent.syncStatus" />
        </div>
      </div>
    </aside>
    <section class="dashboard__main">
      <header class="dashboard__topbar">
        <div class="dashboard__topbar-info">
          <h2 class="dashboard__topbar-name">{{ activeAgent?.displayName || '未选择 Agent' }}</h2>
          <span v-if="activeAgent" class="dashboard__topbar-path">📁 {{ activeAgent.configDir }}</span>
        </div>
        <n-button type="primary" :loading="syncStore.syncing" :disabled="!activeAgent" @click="syncStore.syncAll">全部同步</n-button>
      </header>
      <div v-if="!activeAgent" class="dashboard__welcome">
        <div class="dashboard__welcome-icon">⚡</div>
        <h3 class="dashboard__welcome-title">选择一个 Agent 开始</h3>
        <p class="dashboard__welcome-desc">从左侧选择 AI 助手，或在设置页添加新的 Agent</p>
      </div>
      <div v-else class="dashboard__content">
        <div class="dashboard__cards">
          <div class="dashboard__card">
            <div class="dashboard__card-header"><span class="dashboard__card-title">同步状态</span></div>
            <div class="dashboard__card-body">
              <div class="dashboard__stat">
                <span class="dashboard__stat-label">上次同步</span>
                <span class="dashboard__stat-value">{{ activeAgent.lastSyncAt ? new Date(activeAgent.lastSyncAt).toLocaleString() : '从未' }}</span>
              </div>
              <div class="dashboard__stat">
                <span class="dashboard__stat-label">跟踪文件</span>
                <span class="dashboard__stat-value">{{ activeAgent.trackedFileCount }} 个</span>
              </div>
            </div>
          </div>
          <div class="dashboard__card">
            <div class="dashboard__card-header"><span class="dashboard__card-title">当前人格</span></div>
            <div class="dashboard__card-body">
              <div class="dashboard__stat">
                <span class="dashboard__stat-label">激活人格</span>
                <span class="dashboard__stat-value">{{ activeAgent.currentPersona || '未激活' }}</span>
              </div>
            </div>
            <div class="dashboard__card-footer">
              <n-button size="small" secondary @click="showSwitchDialog = true">切换</n-button>
              <n-button size="small" secondary @click="showSaveDialog = true">保存当前</n-button>
            </div>
          </div>
        </div>
        <div class="dashboard__files">
          <div class="dashboard__files-header">
            <span class="dashboard__card-title">跟踪文件</span>
            <span class="dashboard__files-count">{{ trackedFiles.length }} 个文件</span>
          </div>
          <n-data-table :columns="fileColumns" :data="trackedFiles" :bordered="false" :loading="loadingFiles" size="small" flex-height style="height: 100%" />
        </div>
      </div>
    </section>
    <ConflictDialog :show="conflictShow" :payload="conflictPayload" :resolving="conflictResolving" @resolve="onConflictResolve" @cancel="onConflictCancel" />
    <n-modal v-model:show="showSwitchDialog" preset="card" title="切换人格" style="width: 400px">
      <n-empty v-if="agentPersonalities.length === 0" description="暂无已保存的人格" />
      <n-list v-else bordered>
        <n-list-item v-for="p in agentPersonalities" :key="p.name" class="dashboard__persona-item" @click="!switching && onSwitchPersona(p.name)">
          <n-thing>
            <template #header>{{ p.displayName }}</template>
            <template #description>{{ p.files.length }} 文件 · {{ Math.round(p.sizeBytes / 1024) }} KB</template>
          </n-thing>
        </n-list-item>
      </n-list>
      <div v-if="switching" class="dashboard__switching">切换中...</div>
    </n-modal>
    <n-modal v-model:show="showSaveDialog" preset="card" title="保存当前为人格" style="width: 400px">
      <n-input v-model:value="savePersonaName" placeholder="人格名称，如 work-mode" />
      <template #footer>
        <n-space justify="end">
          <n-button @click="showSaveDialog = false">取消</n-button>
          <n-button type="primary" :loading="saving" @click="onSavePersona">保存</n-button>
        </n-space>
      </template>
    </n-modal>
  </div>
</template>

<style scoped>
.dashboard { display: flex; height: 100%; }
.dashboard__sidebar { width: 240px; flex-shrink: 0; background: var(--bg-sidebar); display: flex; flex-direction: column; overflow: hidden; }
.dashboard__sidebar-header { display: flex; align-items: center; justify-content: space-between; padding: 16px 20px; }
.dashboard__sidebar-title { font-size: 11px; font-weight: 700; color: var(--text-sidebar-muted); text-transform: uppercase; letter-spacing: 1px; cursor: pointer; }
.dashboard__sidebar-count { font-size: 11px; color: var(--text-sidebar-muted); background: var(--bg-sidebar-hover); padding: 2px 8px; border-radius: 10px; }
.dashboard__dev { padding: 0 20px 4px; font-size: 10px; color: var(--text-sidebar-muted); }
.dashboard__dev-ok { color: var(--color-success); }
.dashboard__agent-list { flex: 1; min-height: 0; overflow-y: auto; padding: 0 8px; }
.dashboard__agent-empty { text-align: center; padding: 40px 20px; color: var(--text-sidebar-muted); }
.dashboard__agent-empty-icon { font-size: 32px; margin-bottom: 8px; }
.dashboard__agent-empty p { margin: 0 0 8px 0; font-size: 13px; }
.dashboard__agent-item { display: flex; align-items: center; justify-content: space-between; padding: 10px 12px; border-radius: var(--radius-sm); cursor: pointer; transition: var(--transition); margin-bottom: 2px; }
.dashboard__agent-item:hover { background: var(--bg-sidebar-hover); }
.dashboard__agent-item--active { background: var(--bg-sidebar-active); }
.dashboard__agent-info { display: flex; flex-direction: column; gap: 2px; }
.dashboard__agent-name { font-size: 13px; font-weight: 500; color: var(--text-sidebar); }
.dashboard__agent-persona { font-size: 11px; color: var(--text-sidebar-muted); }
.dashboard__main { flex: 1; display: flex; flex-direction: column; overflow: hidden; background: var(--bg-app); }
.dashboard__topbar { display: flex; justify-content: space-between; align-items: center; padding: 16px 24px; background: var(--bg-card); border-bottom: 1px solid var(--border-light); }
.dashboard__topbar-info { display: flex; flex-direction: column; gap: 2px; }
.dashboard__topbar-name { margin: 0; font-size: 18px; font-weight: 700; color: var(--text-primary); }
.dashboard__topbar-path { font-size: 12px; color: var(--text-tertiary); }
.dashboard__welcome { flex: 1; display: flex; flex-direction: column; align-items: center; justify-content: center; color: var(--text-tertiary); }
.dashboard__welcome-icon { font-size: 48px; margin-bottom: 16px; }
.dashboard__welcome-title { margin: 0 0 8px 0; font-size: 18px; color: var(--text-secondary); }
.dashboard__welcome-desc { margin: 0; font-size: 14px; }
.dashboard__content { flex: 1; display: flex; flex-direction: column; gap: var(--space-md); padding: var(--space-lg); overflow: auto; }
.dashboard__cards { display: flex; gap: var(--space-md); }
.dashboard__card { flex: 1; background: var(--bg-card); border: 1px solid var(--border-light); border-radius: var(--radius-md); overflow: hidden; }
.dashboard__card-header { padding: 12px 16px 0; }
.dashboard__card-title { font-size: 13px; font-weight: 600; color: var(--text-secondary); }
.dashboard__card-body { padding: 12px 16px; display: flex; gap: 32px; }
.dashboard__stat { display: flex; flex-direction: column; gap: 4px; }
.dashboard__stat-label { font-size: 11px; color: var(--text-tertiary); }
.dashboard__stat-value { font-size: 15px; font-weight: 600; color: var(--text-primary); }
.dashboard__card-footer { display: flex; gap: var(--space-sm); padding: 8px 16px 12px; border-top: 1px solid var(--border-light); }
.dashboard__files { flex: 1; min-height: 200px; background: var(--bg-card); border: 1px solid var(--border-light); border-radius: var(--radius-md); display: flex; flex-direction: column; overflow: hidden; }
.dashboard__files-header { display: flex; justify-content: space-between; align-items: center; padding: 12px 16px; border-bottom: 1px solid var(--border-light); }
.dashboard__files-count { font-size: 12px; color: var(--text-tertiary); }
.dashboard__persona-item { cursor: pointer; transition: var(--transition); }
.dashboard__persona-item:hover { background: var(--bg-hover); }
.dashboard__switching { text-align: center; padding: 12px; color: var(--text-tertiary); }
</style>