<script setup lang="ts">
import { ref, onMounted, computed } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useRoute, useRouter } from 'vue-router'
import {
  NTabs,
  NTab,
  NConfigProvider,
  NMessageProvider,
  NDialogProvider
} from 'naive-ui'
import Onboarding from '@/components/Onboarding.vue'

const route = useRoute()
const router = useRouter()

const activeTab = computed(() => route.name as string)
function onTabChange(name: string) {
  router.push({ name })
}

const initialized = ref<boolean | null>(null)

onMounted(async () => {
  try {
    initialized.value = await invoke<boolean>('is_repo_initialized')
  } catch (e) {
    console.error('检查初始化状态失败:', e)
    initialized.value = false
  }
})

function onOnboardingCompleted() {
  initialized.value = true
}
</script>

<template>
  <n-config-provider>
    <n-message-provider>
      <n-dialog-provider>
        <div v-if="initialized === null" class="app__loading">
          <div class="app__loading-spinner" />
        </div>

        <Onboarding
          v-else-if="initialized === false"
          @completed="onOnboardingCompleted"
        />

        <div v-else class="app">
          <header class="app__header">
            <div class="app__brand">
              <div class="app__logo-icon">
                <svg width="20" height="20" viewBox="0 0 20 20" fill="none">
                  <path d="M10 2L3 6v8l7 4 7-4V6l-7-4z" stroke="white" stroke-width="1.5" stroke-linejoin="round"/>
                  <path d="M10 2v16M3 6l7 4 7-4M3 14l7-4 7 4" stroke="white" stroke-width="1.5" stroke-linejoin="round" opacity="0.6"/>
                </svg>
              </div>
              <span class="app__logo-text">AgentSync</span>
            </div>
            <n-tabs
              :value="activeTab"
              type="segment"
              size="small"
              @update:value="onTabChange"
              class="app__tabs"
            >
              <n-tab name="dashboard">仪表盘</n-tab>
              <n-tab name="personalities">人格管理</n-tab>
              <n-tab name="settings">设置</n-tab>
            </n-tabs>
          </header>
          <main class="app__main">
            <router-view />
          </main>
        </div>
      </n-dialog-provider>
    </n-message-provider>
  </n-config-provider>
</template>

<style>
html, body, #app {
  margin: 0;
  padding: 0;
  height: 100%;
}

.app {
  display: flex;
  flex-direction: column;
  height: 100vh;
  background: var(--bg-app);
}

.app__loading {
  height: 100vh;
  display: flex;
  align-items: center;
  justify-content: center;
}
.app__loading-spinner {
  width: 32px;
  height: 32px;
  border: 3px solid var(--border-light);
  border-top-color: var(--brand-primary);
  border-radius: 50%;
  animation: spin 0.6s linear infinite;
}
@keyframes spin {
  to { transform: rotate(360deg); }
}

.app__header {
  display: flex;
  align-items: center;
  padding: 0 var(--space-lg);
  height: 56px;
  background: var(--bg-card);
  border-bottom: 1px solid var(--border-light);
  box-shadow: var(--shadow-sm);
  flex-shrink: 0;
}

.app__brand {
  display: flex;
  align-items: center;
  gap: 10px;
  margin-right: var(--space-xl);
}

.app__logo-icon {
  width: 32px;
  height: 32px;
  border-radius: var(--radius-sm);
  background: var(--brand-gradient);
  display: flex;
  align-items: center;
  justify-content: center;
  box-shadow: 0 2px 8px rgba(99, 102, 241, 0.3);
}

.app__logo-text {
  font-size: 16px;
  font-weight: 700;
  color: var(--text-primary);
  letter-spacing: -0.3px;
}

.app__tabs {
  flex: 1;
}

.app__main {
  flex: 1;
  overflow: hidden;
}
</style>
