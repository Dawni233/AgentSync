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

// onboarding 状态：null=检查中，true=已初始化，false=需 onboarding
const initialized = ref<boolean | null>(null)

onMounted(async () => {
  try {
    initialized.value = await invoke<boolean>('is_repo_initialized')
  } catch (e) {
    console.error('检查初始化状态失败:', e)
    // 检查失败时假设未初始化，让用户走 onboarding
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
        <!-- 检查中 -->
        <div v-if="initialized === null" class="app__loading">
          <span>检查初始化状态...</span>
        </div>

        <!-- 未初始化：显示 Onboarding 向导 -->
        <Onboarding
          v-else-if="initialized === false"
          @completed="onOnboardingCompleted"
        />

        <!-- 已初始化：显示主界面 -->
        <div v-else class="app">
          <header class="app__header">
            <div class="app__brand">
              <span class="app__logo">AgentSync</span>
              <span class="app__version">v0.1.0</span>
            </div>
            <n-tabs
              :value="activeTab"
              type="line"
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
html,
body,
#app {
  margin: 0;
  padding: 0;
  height: 100%;
  font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, 'Helvetica Neue',
    Arial, 'PingFang SC', 'Microsoft YaHei', sans-serif;
  background: #f5f5f5;
  color: #18181c;
}

.app {
  display: flex;
  flex-direction: column;
  height: 100vh;
  background: #fff;
}

.app__loading {
  height: 100vh;
  display: flex;
  align-items: center;
  justify-content: center;
  color: #71717a;
}

.app__header {
  display: flex;
  align-items: center;
  padding: 0 16px;
  border-bottom: 1px solid #e4e4e7;
  background: #fff;
}

.app__brand {
  display: flex;
  align-items: baseline;
  gap: 8px;
  margin-right: 32px;
}

.app__logo {
  font-weight: 600;
  font-size: 16px;
  color: #18181c;
}

.app__version {
  font-size: 11px;
  color: #a1a1aa;
}

.app__tabs {
  flex: 1;
}

.app__main {
  flex: 1;
  overflow: hidden;
}

::-webkit-scrollbar {
  width: 8px;
  height: 8px;
}
::-webkit-scrollbar-track {
  background: transparent;
}
::-webkit-scrollbar-thumb {
  background: #d4d4d8;
  border-radius: 4px;
}
::-webkit-scrollbar-thumb:hover {
  background: #a1a1aa;
}
</style>
