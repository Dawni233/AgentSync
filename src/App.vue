<script setup lang="ts">
import { ref, computed, onMounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useRoute, useRouter } from 'vue-router'
import { useAgentsStore } from '@/stores/agents'
import { useSyncStore } from '@/stores/sync'
import { useToasts } from '@/composables/useToast'
import Onboarding from '@/components/Onboarding.vue'

const route = useRoute()
const router = useRouter()
const agentsStore = useAgentsStore()
const syncStore = useSyncStore()
const toasts = useToasts()

const initialized = ref<boolean | null>(null)
const theme = ref<'light' | 'dark'>('light')

const navItems = [
  {
    view: 'dashboard',
    label: '仪表盘',
    icon: '<rect x="3" y="3" width="7" height="9" rx="1.5"/><rect x="14" y="3" width="7" height="5" rx="1.5"/><rect x="14" y="12" width="7" height="9" rx="1.5"/><rect x="3" y="16" width="7" height="5" rx="1.5"/>'
  },
  {
    view: 'personalities',
    label: '人格管理',
    icon: '<circle cx="9" cy="8" r="3.2"/><path d="M3.5 19a5.5 5.5 0 0 1 11 0"/><path d="M16 6.5a3 3 0 0 1 0 5.8M17.5 19a5 5 0 0 0-3-4.6"/>'
  },
  {
    view: 'settings',
    label: '设置',
    icon: '<circle cx="12" cy="12" r="3"/><path d="M19.4 13.5a1.6 1.6 0 0 0 .3 1.8l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.6 1.6 0 0 0-2.7 1.1V21a2 2 0 1 1-4 0v-.1A1.6 1.6 0 0 0 7 19.4l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1A1.6 1.6 0 0 0 3.6 13.5H3a2 2 0 1 1 0-4h.1A1.6 1.6 0 0 0 4.6 7l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1A1.6 1.6 0 0 0 10 4.6V3a2 2 0 1 1 4 0v.1a1.6 1.6 0 0 0 2.7 1.1l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1a1.6 1.6 0 0 0 1.1 2.7H21a2 2 0 1 1 0 4h-.1a1.6 1.6 0 0 0-1.5 1z"/>'
  }
]

function go(v: string) {
  router.push({ name: v })
}

function applyTheme(t: string) {
  document.documentElement.setAttribute('data-theme', t)
  localStorage.setItem('as-theme', t)
}
function toggleTheme() {
  theme.value = theme.value === 'light' ? 'dark' : 'light'
  applyTheme(theme.value)
}

// 全局同步状态（驱动顶栏胶囊）
const statusMeta: Record<string, { label: string; pulsing: boolean }> = {
  idle: { label: '已同步', pulsing: false },
  syncing: { label: '同步中', pulsing: true },
  pending: { label: '待同步', pulsing: false },
  conflict: { label: '冲突', pulsing: false },
  error: { label: '错误', pulsing: false }
}
const globalStatus = computed(() => {
  if (syncStore.syncing) return 'syncing'
  const ags = agentsStore.agents
  if (ags.some((a) => a.syncStatus === 'conflict')) return 'conflict'
  if (ags.some((a) => a.syncStatus === 'error')) return 'error'
  if (ags.some((a) => a.syncStatus === 'pending')) return 'pending'
  return 'idle'
})
const lastSyncText = computed(() => {
  const times = agentsStore.agents.map((a) => a.lastSyncAt).filter((t): t is number => !!t)
  if (times.length === 0) return '尚未同步'
  const max = Math.max(...times)
  const diff = Date.now() - max
  if (diff < 60_000) return '刚刚'
  if (diff < 3_600_000) return `${Math.floor(diff / 60_000)} 分钟前`
  if (diff < 86_400_000) return `${Math.floor(diff / 3_600_000)} 小时前`
  return new Date(max).toLocaleDateString()
})

onMounted(async () => {
  try {
    initialized.value = await invoke<boolean>('is_repo_initialized')
  } catch {
    initialized.value = false
  }
  try {
    await agentsStore.loadAgents()
  } catch {
    /* 仓库未初始化时忽略 */
  }
  const saved = localStorage.getItem('as-theme') || 'light'
  theme.value = saved as 'light' | 'dark'
  applyTheme(saved)
})

function onOnboardingCompleted() {
  initialized.value = true
}
</script>

<template>
  <div v-if="initialized === null" class="app">
    <div class="detail__empty"><div class="detail__empty-icon">⟳</div><span>正在检查仓库状态…</span></div>
  </div>

  <Onboarding v-else-if="initialized === false" @completed="onOnboardingCompleted" />

  <div v-else class="app">
    <header class="topnav">
      <div class="topnav__left">
        <div class="brand">
          <div class="brand__mark">
            <svg width="18" height="18" viewBox="0 0 20 20" fill="none">
              <path d="M10 2L3 6v8l7 4 7-4V6l-7-4z" stroke="currentColor" stroke-width="1.5" stroke-linejoin="round" />
              <path d="M10 2v16M3 6l7 4 7-4M3 14l7-4 7 4" stroke="currentColor" stroke-width="1.5" stroke-linejoin="round" opacity="0.55" />
            </svg>
          </div>
          <span class="brand__name">AgentSync</span>
        </div>
        <nav class="nav">
          <div
            v-for="item in navItems"
            :key="item.view"
            class="nav__item"
            :class="{ 'is-active': route.name === item.view }"
            role="button"
            tabindex="0"
            :aria-label="`切换到${item.label}`"
            @click="go(item.view)"
            @keydown.enter.prevent="go(item.view)"
            @keydown.space.prevent="go(item.view)"
          >
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" v-html="item.icon" />
            <span>{{ item.label }}</span>
          </div>
        </nav>
      </div>

      <div class="topnav__right">
        <div class="sync-pill" aria-live="polite">
          <span class="sync-pill__dot" :class="{ 'is-pulsing': statusMeta[globalStatus].pulsing }" />
          <div class="sync-pill__txt">
            <b>{{ statusMeta[globalStatus].label }}</b>
            <span>{{ lastSyncText }}</span>
          </div>
        </div>
        <button class="theme-toggle" @click="toggleTheme" :aria-label="theme === 'light' ? '切换到暗色' : '切换到亮色'">
          <svg v-if="theme === 'light'" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"><path d="M21 12.8A9 9 0 1 1 11.2 3a7 7 0 0 0 9.8 9.8z" /></svg>
          <svg v-else viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="4.2"/><path d="M12 2v2.5M12 19.5V22M4.2 4.2l1.8 1.8M18 18l1.8 1.8M2 12h2.5M19.5 12H22M4.2 19.8l1.8-1.8M18 6l1.8-1.8"/></svg>
          <span>{{ theme === 'light' ? '亮色' : '暗色' }}</span>
        </button>
      </div>
    </header>

    <main class="main">
      <router-view />
    </main>

    <div class="toast-root">
      <div v-for="t in toasts" :key="t.id" class="toast">
        <span class="toast__dot" />
        <span>{{ t.msg }}</span>
      </div>
    </div>
  </div>
</template>
