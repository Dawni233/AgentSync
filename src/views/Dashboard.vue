<script setup lang="ts">
import { ref, onMounted, onUnmounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import {
  NCard,
  NButton,
  NSpace,
  NEmpty,
  NStatistic,
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
    <!-- 左侧边栏：Agent 列表（深色） -->
    <aside class="dashboard__sidebar">
      <div class="dashboard__sidebar-header">
        <span class="dashboard__sidebar-title" @click="devPing">Agents</span>
        <!-- 开发自检结果（仅 dev 期间可见，不占用正式布局） -->
        <div v-if="devPingResult" class="dashboard__dev">
          <div>IPC: <span class="dashboard__dev-ok">✓</span></div>
          <div :title="devAppDataDir" class="dashboard__dev-path">
            {{ devAppDataDir }}
          </div>
        </div>
      </div>
      <div class="dashboard__agent-list">
        <n-empty v-if="agentsStore.agents.length === 0" size="small" description="无 Agent">
          <template #extra>
            <n-button text size="small" @click="agentsStore.loadAgents">刷新</n-button>
          </template>
        </n-empty>
        <div
          v-for="agent in agentsStore.agents"
          :key="agent.id"
          class="dashboard__agent-item"
          :class="{ 'dashboard__agent-item--active': activeAgent?.id === agent.id }"
          @click="onSelectAgent(agent)"
        >
          <span class="dashboard__agent-name">{{ agent.displayName }}</span>
          <span class="dashboard__agent-persona">{{ agent.currentPersona || '—' }}</span>
          <SyncStatusBadge :status="agent.syncStatus" />
        </div>
      </div>
    </aside>

    <!-- 右侧主区域 -->
    <section class="dashboard__main">
      <!-- 顶栏：Agent 名称 + 配置路径 + 全部同步按钮 -->
      <header class="dashboard__topbar">
        <div class="dashboard__topbar-info">
          <span class="dashboard__topbar-name">
            {{ activeAgent?.displayName || '未选择 Agent' }}
          </span>
          <span v-if="activeAgent" class="dashboard__topbar-path">
            {{ activeAgent.configDir }}
          </span>
        </div>
        <n-button type="primary" :loading="syncStore.syncing" @click="syncStore.syncAll">
          全部同步
        </n-button>
      </header>

      <!-- 无选中 Agent 时的引导 -->
      <div v-if="!activeAgent" class="dashboard__empty">
        <n-empty description="请从左侧选择一个 Agent，或在设置页注册新 Agent" />
      </div>

      <!-- 状态卡片 + 文件列表 -->
      <div v-else class="dashboard__content">
        <!-- 上方：两张状态卡片 -->
        <n-space class="dashboard__cards">
          <n-card title="同步状态" size="small" class="dashboard__card">
            <n-statistic label="上次同步" :value="activeAgent.lastSyncAt ? new Date(activeAgent.lastSyncAt).toLocaleString() : '从未'" />
            <n-statistic label="跟踪文件数" :value="activeAgent.trackedFileCount" />
          </n-card>
          <n-card title="当前人格" size="small" class="dashboard__card">
            <n-statistic
              label="激活人格"
              :value="activeAgent.currentPersona || '未激活'"
            />
            <template #footer>
              <n-space>
                <n-button size="small" @click="showSwitchDialog = true">切换</n-button>
                <n-button size="small" @click="showSaveDialog = true">保存当前</n-button>
              </n-space>
            </template>
          </n-card>
        </n-space>

        <!-- 下方：文件列表 -->
        <n-card title="跟踪文件" size="small" class="dashboard__files">
          <n-data-table
            :columns="fileColumns"
            :data="trackedFiles"
            :bordered="false"
            size="small"
            flex-height
            style="height: 100%"
          />
        </n-card>
      </div>
    </section>

    <!-- 冲突弹窗 -->
    <ConflictDialog
      :show="conflictShow"
      :payload="conflictPayload"
      :resolving="conflictResolving"
      @resolve="onConflictResolve"
      @cancel="onConflictCancel"
    />

    <!-- 切换人格弹窗 -->
    <n-modal
      v-model:show="showSwitchDialog"
      preset="card"
      title="切换人格"
      style="width: 400px"
    >
      <n-empty v-if="agentPersonalities.length === 0" description="暂无已保存的人格" />
      <n-list v-else bordered>
        <n-list-item
          v-for="p in agentPersonalities"
          :key="p.name"
          class="dashboard__persona-item"
          @click="!switching && onSwitchPersona(p.name)"
        >
          <n-thing>
            <template #header>{{ p.displayName }}</template>
            <template #description>{{ p.files.length }} 文件 · {{ Math.round(p.sizeBytes / 1024) }} KB</template>
          </n-thing>
        </n-list-item>
      </n-list>
      <div v-if="switching" class="dashboard__switching">切换中...</div>
    </n-modal>

    <!-- 保存人格弹窗 -->
    <n-modal
      v-model:show="showSaveDialog"
      preset="card"
      title="保存当前为人格"
      style="width: 400px"
    >
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
.dashboard {
  display: flex;
  height: 100%;
}

/* 左侧边栏：深色 */
.dashboard__sidebar {
  width: 240px;
  flex-shrink: 0;
  background: #1e1e1e;
  color: #e4e4e7;
  display: flex;
  flex-direction: column;
  border-right: 1px solid #27272a;
  overflow: hidden;
}

/* 深色侧边栏内按钮强制浅色文字 */
.dashboard__sidebar :deep(.n-button) {
  color: #e4e4e7 !important;
}
.dashboard__sidebar :deep(.n-button:hover) {
  color: #fff !important;
}
.dashboard__sidebar-header {
  padding: 16px;
  border-bottom: 1px solid #27272a;
}
.dashboard__sidebar-title {
  font-size: 13px;
  font-weight: 600;
  color: #a1a1aa;
  text-transform: uppercase;
  letter-spacing: 0.5px;
  cursor: pointer;
}
.dashboard__dev {
  margin-top: 8px;
  font-size: 11px;
  color: #71717a;
}
.dashboard__dev-ok {
  color: #22c55e;
}
.dashboard__dev-path {
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  margin-top: 2px;
}

.dashboard__agent-list {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  padding: 8px;
}
.dashboard__agent-item {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 10px 12px;
  border-radius: 6px;
  cursor: pointer;
  transition: background 0.15s;
}
.dashboard__agent-item:hover {
  background: #27272a;
}
.dashboard__agent-item--active {
  background: #3f3f46;
}
.dashboard__agent-name {
  flex: 1;
  font-size: 13px;
  color: #e4e4e7;
}
.dashboard__agent-persona {
  font-size: 11px;
  color: #71717a;
}

/* 右侧主区域：浅色 */
.dashboard__main {
  flex: 1;
  display: flex;
  flex-direction: column;
  overflow: hidden;
  background: #fff;
}
.dashboard__topbar {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 12px 20px;
  border-bottom: 1px solid #e4e4e7;
}
.dashboard__topbar-info {
  display: flex;
  flex-direction: column;
  gap: 2px;
}
.dashboard__topbar-name {
  font-size: 15px;
  font-weight: 600;
  color: #18181c;
}
.dashboard__topbar-path {
  font-size: 12px;
  color: #a1a1aa;
}

.dashboard__empty {
  flex: 1;
  display: flex;
  align-items: center;
  justify-content: center;
}
.dashboard__content {
  flex: 1;
  display: flex;
  flex-direction: column;
  gap: 12px;
  padding: 16px 20px;
  overflow: auto;
}
.dashboard__cards {
  flex-shrink: 0;
}
.dashboard__card {
  min-width: 240px;
}
.dashboard__persona-item {
  cursor: pointer;
  transition: background 0.15s;
}
.dashboard__persona-item:hover {
  background: #f4f4f5;
}
.dashboard__switching {
  text-align: center;
  padding: 12px;
  color: #71717a;
}
.dashboard__files {
  flex: 1;
  min-height: 200px;
}
</style>
