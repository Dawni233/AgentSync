<script setup lang="ts">
import { ref, onMounted, onUnmounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useAgentsStore } from '@/stores/agents'
import { useSyncStore } from '@/stores/sync'
import { useSyncEvents } from '@/composables/useSync'
import { usePersonalities } from '@/composables/usePersonalities'
import { showToast } from '@/composables/useToast'
import type { ResolutionOption } from '@/components/ConflictDialog.vue'
import SyncStatusBadge from '@/components/SyncStatusBadge.vue'
import ConflictDialog from '@/components/ConflictDialog.vue'
import type { Agent, ConflictDetectedPayload, Persona } from '@/types'

const agentsStore = useAgentsStore()
const syncStore = useSyncStore()
const toast = showToast

const activeAgent = ref<Agent | null>(null)
const conflictShow = ref(false)
const conflictPayload = ref<ConflictDetectedPayload | null>(null)
const conflictResolving = ref(false)
let unlistenEvents: (() => void) | null = null

onMounted(async () => {
  try {
    await agentsStore.loadAgents()
    if (!activeAgent.value && agentsStore.agents.length > 0) {
      await onSelectAgent(agentsStore.agents[0])
    }
  } catch (e) {
    toast(`加载 agents 失败: ${e}`)
  }
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

// 人格切换
const { listPersonalities, switchPersonality } = usePersonalities()
const agentPersonalities = ref<Persona[]>([])
const switching = ref(false)

async function loadAgentPersonalities(agentId: string) {
  try {
    agentPersonalities.value = await listPersonalities(agentId)
  } catch {
    agentPersonalities.value = []
  }
}

async function onSwitchPersona(name: string) {
  if (!activeAgent.value) return
  switching.value = true
  try {
    await switchPersonality(activeAgent.value.id, name)
    toast(`已切换到 ${name}`)
    await agentsStore.loadAgents()
    const updated = agentsStore.agents.find((a) => a.id === activeAgent.value?.id)
    if (updated) activeAgent.value = updated
  } catch (e) {
    toast(`切换失败: ${e}`)
  } finally {
    switching.value = false
  }
}

async function onConflictResolve(option: ResolutionOption) {
  conflictResolving.value = true
  try {
    await syncStore.resolveConflict({ type: option })
    conflictShow.value = false
    conflictPayload.value = null
    await agentsStore.loadAgents()
    toast('冲突已解决')
  } catch (e) {
    toast(`解决冲突失败: ${e}`)
  } finally {
    conflictResolving.value = false
  }
}
function onConflictCancel() {
  const option: ResolutionOption = conflictPayload.value?.conflictType === 'L1' ? 'L1Cancel' : 'L2Cancel'
  onConflictResolve(option)
}

// 文件列表
const trackedFiles = ref<{ name: string; size: string; mtime: string }[]>([])
const loadingFiles = ref(false)
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
    toast(`加载文件列表失败: ${e}`)
    trackedFiles.value = []
  } finally {
    loadingFiles.value = false
  }
}

async function onSyncAll() {
  try {
    await syncStore.syncAll()
    await agentsStore.loadAgents()
    toast('已同步 · 全部')
  } catch (e) {
    toast(`同步失败: ${e}`)
  }
}
</script>

<template>
  <div class="dash">
    <!-- 左：Agent 列表 -->
    <aside class="agent-rail">
      <div class="agent-rail__head">
        <span class="agent-rail__title">Agents</span>
        <span class="agent-rail__count">{{ agentsStore.agents.length }}</span>
      </div>
      <div class="agent-list">
        <div v-if="agentsStore.agents.length === 0" class="detail__empty">
          <div class="detail__empty-icon">🤖</div>
          <span>暂无 Agent</span>
        </div>
        <div
          v-for="agent in agentsStore.agents"
          :key="agent.id"
          class="agent"
          :class="{ 'is-active': activeAgent?.id === agent.id }"
          role="button"
          tabindex="0"
          @click="onSelectAgent(agent)"
          @keydown.enter.prevent="onSelectAgent(agent)"
          @keydown.space.prevent="onSelectAgent(agent)"
        >
          <div class="agent__glyph" :style="{ background: agent.accentColor || '#5B4FE9' }">
            {{ (agent.displayName || agent.id).charAt(0).toUpperCase() }}
          </div>
          <div class="agent__body">
            <div class="agent__name">
              <span>{{ agent.displayName }}</span>
              <span v-if="agent.currentPersona" class="agent__cur">激活</span>
            </div>
            <div class="agent__sub">{{ agent.configDir }}</div>
          </div>
          <SyncStatusBadge :status="agent.syncStatus" />
        </div>
      </div>
    </aside>

    <!-- 右：详情 -->
    <section class="dash__col">
      <div class="subbar">
        <div>
          <h2 class="subbar__title">{{ activeAgent ? activeAgent.displayName : '仪表盘' }}</h2>
          <div class="subbar__crumb">工作台 / 概览</div>
        </div>
        <div class="subbar__actions">
          <button class="btn btn--primary" :disabled="!activeAgent || syncStore.syncing || loadingFiles" @click="onSyncAll">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"><path d="M21 12a9 9 0 0 1-9 9 9 9 0 0 1-8-5M3 12a9 9 0 0 1 9-9 9 9 0 0 1 8 5"/><path d="M21 4v4h-4M3 20v-4h4"/></svg>
            全部同步
          </button>
        </div>
      </div>

      <div class="detail">
        <div v-if="!activeAgent" class="detail__empty">
          <div class="detail__empty-icon">⚡</div>
          <div class="empty__title">选择一个 Agent 开始</div>
          <div>从左侧选择 AI 助手，或在设置页添加新 Agent</div>
        </div>

        <template v-else>
          <div class="summary">
            <div class="summary__top">
              <div>
                <h3 class="summary__name">{{ activeAgent.displayName }}</h3>
                <div class="summary__path">📁 {{ activeAgent.configDir }}</div>
              </div>
            </div>
            <div class="stat-row">
              <div class="stat">
                <span class="stat__label">上次同步</span>
                <span class="stat__value stat__value--sm">{{ activeAgent.lastSyncAt ? new Date(activeAgent.lastSyncAt).toLocaleString() : '从未' }}</span>
              </div>
              <div class="stat">
                <span class="stat__label">跟踪文件</span>
                <span class="stat__value stat__value--sm">{{ activeAgent.trackedFileCount }}</span>
              </div>
              <div class="stat">
                <span class="stat__label">当前人格</span>
                <span class="stat__value stat__value--sm">{{ activeAgent.currentPersona || '未激活' }}</span>
              </div>
            </div>

            <div class="persona-bar">
              <span class="persona-bar__label">人格快切</span>
              <span
                v-for="p in agentPersonalities"
                :key="p.name"
                class="chip"
                :class="{ 'is-current': p.name === activeAgent.currentPersona }"
                role="button"
                tabindex="0"
                @click="onSwitchPersona(p.name)"
                @keydown.enter.prevent="onSwitchPersona(p.name)"
                @keydown.space.prevent="onSwitchPersona(p.name)"
              >{{ p.name }}</span>
            </div>
          </div>

          <div class="panel">
            <div class="panel__head">
              <span class="panel__title">跟踪文件</span>
              <span class="panel__meta">{{ trackedFiles.length }} 个文件</span>
            </div>
            <table class="filedata">
              <thead>
                <tr><th>文件名</th><th>大小</th><th>最近变更</th></tr>
              </thead>
              <tbody>
                <tr v-for="f in trackedFiles" :key="f.name">
                  <td class="mono">{{ f.name }}</td>
                  <td>{{ f.size }}</td>
                  <td>{{ f.mtime }}</td>
                </tr>
                <tr v-if="trackedFiles.length === 0">
                  <td colspan="3" style="text-align: center; color: var(--ink-3)">
                    {{ loadingFiles ? '加载中…' : '无文件' }}
                  </td>
                </tr>
              </tbody>
            </table>
          </div>
        </template>
      </div>
    </section>

    <ConflictDialog
      :show="conflictShow"
      :payload="conflictPayload"
      :resolving="conflictResolving"
      @resolve="onConflictResolve"
      @cancel="onConflictCancel"
    />
  </div>
</template>

<style scoped>
.dash__col { display: flex; flex-direction: column; min-height: 0; overflow: hidden; }
.detail { flex: 1; min-height: 0; }
</style>
