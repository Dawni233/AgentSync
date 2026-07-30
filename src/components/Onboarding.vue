<script setup lang="ts">
import { ref, reactive } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import {
  NCard,
  NForm,
  NFormItem,
  NInput,
  NSelect,
  NButton,
  NSpace,
  NTag,
  NSteps,
  NStep,
  NCheckbox,
  NRadio,
  NRadioGroup,
  NAlert,
  NSpin,
  useMessage
} from 'naive-ui'

const emit = defineEmits<{ completed: [] }>()
const message = useMessage()

const currentStep = ref(1)

// 步骤 1：凭据
const credentials = reactive({
  repoUrl: '',
  platform: 'gitee' as 'gitee' | 'github',
  patToken: ''
})

const platformOptions = [
  { label: 'Gitee', value: 'gitee' },
  { label: 'GitHub', value: 'github' }
]

const authTesting = ref(false)
const authVerified = ref(false)

async function verifyAuth() {
  if (!credentials.repoUrl || !credentials.patToken) {
    message.warning('请填写仓库 URL 和 PAT')
    return
  }
  authTesting.value = true
  authVerified.value = false
  try {
    const ok = await invoke<boolean>('test_git_auth', {
      url: credentials.repoUrl,
      token: credentials.patToken
    })
    authVerified.value = ok
    if (ok) {
      message.success('凭据验证通过')
    } else {
      message.error('凭据验证失败')
    }
  } catch (e) {
    message.error(`验证失败: ${e}`)
  } finally {
    authTesting.value = false
  }
}

function goToStep2() {
  if (!authVerified.value) {
    message.warning('请先验证凭据')
    return
  }
  currentStep.value = 2
}

// 步骤 2：预置 agent + 导入策略
const presetAgents = [
  { id: 'workbuddy', label: 'WorkBuddy', desc: '~/.workbuddy', color: '#42b883' },
  { id: 'claude-code', label: 'Claude Code', desc: '~/.claude', color: '#d97706' },
  { id: 'cursor', label: 'Cursor', desc: '~/.cursor', color: '#6366f1' },
  { id: 'codex', label: 'Codex', desc: '~/.codex', color: '#10b981' },
  { id: 'zcode', label: 'ZCode', desc: '~/.zcode', color: '#8b5cf6' },
  { id: 'qoder', label: 'Qoder', desc: '~/.qoderworkcn', color: '#f59e0b' },
  { id: 'openclaw', label: 'OpenClaw', desc: '~/.openclaw', color: '#ef4444' },
  { id: 'qwenpaw', label: 'QwenPaw', desc: '~/.qwenpaw', color: '#3b82f6' }
]
const selectedPresets = ref<string[]>(['workbuddy', 'claude-code', 'cursor', 'codex', 'zcode', 'qoder', 'openclaw', 'qwenpaw'])

// 导入策略：当本地和远程都有配置时如何处理
const importStrategy = ref<'auto' | 'preferLocal' | 'preferRemote'>('auto')

const strategyOptions = [
  {
    label: '自动判断（推荐）',
    value: 'auto',
    desc: '本地有配置则上传，本地无配置则从远程下载，两者都有时默认保留本地'
  },
  {
    label: '优先本地',
    value: 'preferLocal',
    desc: '本地配置覆盖远程（适用于：本机已有配置，想推送到其他设备）'
  },
  {
    label: '优先远程',
    value: 'preferRemote',
    desc: '远程配置覆盖本地（适用于：想用其他设备的配置覆盖本机）'
  }
]

const initializing = ref(false)
const initResult = ref<InitAppResult | null>(null)
const initError = ref<string | null>(null)

type InitAppResult = {
  success: boolean
  errorMessage?: string
  importedAgents: Array<{
    agentId: string
    localConfigExists: boolean
    currentHasContent: boolean
    strategy: 'LocalToCurrent' | 'CurrentToLocal' | 'Empty' | 'Skipped'
  }>
}

async function startInit() {
  initializing.value = true
  initResult.value = null
  initError.value = null
  try {
    const result = await invoke<InitAppResult>('init_app', {
      repoUrl: credentials.repoUrl,
      platform: credentials.platform,
      patToken: credentials.patToken,
      presetAgentIds: selectedPresets.value,
      importStrategy: importStrategy.value
    })
    initResult.value = result
    if (result.success) {
      message.success('初始化完成')
      currentStep.value = 3
    } else {
      initError.value = result.errorMessage || '初始化失败'
      message.error(initError.value)
    }
  } catch (e) {
    initError.value = String(e)
    message.error(`初始化失败: ${e}`)
  } finally {
    initializing.value = false
  }
}

function finish() {
  emit('completed')
}

function retryInit() {
  initError.value = null
  initResult.value = null
  currentStep.value = 2
}

const strategyText: Record<string, string> = {
  LocalToCurrent: '本地配置已上传到远程',
  CurrentToLocal: '远程配置已下载到本地',
  Empty: '创建空配置目录',
  Skipped: '本地无配置，已跳过'
}
</script>

<template>
  <div class="onboarding">
    <div class="onboarding__container">
      <div class="onboarding__logo">
        <svg width="28" height="28" viewBox="0 0 20 20" fill="none">
          <path d="M10 2L3 6v8l7 4 7-4V6l-7-4z" stroke="white" stroke-width="1.5" stroke-linejoin="round"/>
          <path d="M10 2v16M3 6l7 4 7-4M3 14l7-4 7 4" stroke="white" stroke-width="1.5" stroke-linejoin="round" opacity="0.6"/>
        </svg>
      </div>
      <h1 class="onboarding__title">AgentSync</h1>
      <p class="onboarding__subtitle">配置 Git 仓库以开始同步你的 AI 助手配置</p>

      <n-steps :current="currentStep" class="onboarding__steps">
        <n-step title="仓库凭据" />
        <n-step title="预置 Agent" />
        <n-step title="完成" />
      </n-steps>

      <!-- 步骤 1：凭据 -->
      <n-card v-if="currentStep === 1" class="onboarding__card">
        <n-form label-placement="left" :label-width="100">
          <n-form-item label="仓库 URL">
            <n-input
              v-model:value="credentials.repoUrl"
              placeholder="https://gitee.com/user/workbuddy-sync.git"
            />
          </n-form-item>
          <n-form-item label="平台">
            <n-select v-model:value="credentials.platform" :options="platformOptions" />
          </n-form-item>
          <n-form-item label="访问令牌">
            <n-space align="center">
              <n-input
                v-model:value="credentials.patToken"
                type="password"
                show-password-on="click"
                placeholder="Personal Access Token"
                style="width: 360px"
              />
              <n-button :loading="authTesting" @click="verifyAuth">验证</n-button>
              <n-tag v-if="authVerified" type="success">已验证</n-tag>
            </n-space>
          </n-form-item>
        </n-form>

        <n-alert type="info" :bordered="false" class="onboarding__tip">
          请使用私有仓库同步。PAT 需有仓库读写权限。
        </n-alert>

        <div class="onboarding__actions">
          <n-button type="primary" :disabled="!authVerified" @click="goToStep2">
            下一步
          </n-button>
        </div>
      </n-card>

      <!-- 步骤 2：预置 agent + 导入策略 -->
      <n-card v-else-if="currentStep === 2" class="onboarding__card">
        <p class="onboarding__step-desc">
          选择要预置的 AI 助手。应用会检测本地配置目录并自动导入。
        </p>

        <div class="onboarding__presets">
          <label
            v-for="agent in presetAgents"
            :key="agent.id"
            class="onboarding__preset-item"
            :class="{ 'onboarding__preset-item--active': selectedPresets.includes(agent.id) }"
          >
            <n-checkbox
              :checked="selectedPresets.includes(agent.id)"
              @update:checked="(v) => {
                if (v) selectedPresets.push(agent.id)
                else selectedPresets = selectedPresets.filter(id => id !== agent.id)
              }"
            />
            <span class="onboarding__preset-name" :style="{ color: agent.color }">
              {{ agent.label }}
            </span>
            <span class="onboarding__preset-path">{{ agent.desc }}</span>
          </label>
        </div>

        <!-- 导入策略选择 -->
        <div class="onboarding__strategy">
          <div class="onboarding__strategy-title">导入策略</div>
          <div class="onboarding__strategy-desc">
            当本地和远程都有配置时，如何处理冲突：
          </div>
          <n-radio-group v-model:value="importStrategy" class="onboarding__strategy-group">
            <div
              v-for="opt in strategyOptions"
              :key="opt.value"
              class="onboarding__strategy-item"
            >
              <n-radio :value="opt.value">{{ opt.label }}</n-radio>
              <div class="onboarding__strategy-item-desc">{{ opt.desc }}</div>
            </div>
          </n-radio-group>
        </div>

        <div class="onboarding__actions">
          <n-button @click="currentStep = 1">上一步</n-button>
          <n-button
            type="primary"
            :loading="initializing"
            :disabled="selectedPresets.length === 0"
            @click="startInit"
          >
            开始初始化
          </n-button>
        </div>
      </n-card>

      <!-- 步骤 3：完成/错误/加载中 -->
      <n-card v-else class="onboarding__card">
        <!-- 加载中 -->
        <div v-if="initializing" class="onboarding__loading">
          <n-spin size="large" />
          <p class="onboarding__loading-text">正在初始化，请稍候...</p>
          <p class="onboarding__loading-hint">（clone 仓库 + 导入配置 + 推送到远程）</p>
        </div>

        <!-- 错误 -->
        <div v-else-if="initError" class="onboarding__error">
          <p class="onboarding__error-title">✗ 初始化失败</p>
          <n-alert type="error" :bordered="false">{{ initError }}</n-alert>
          <div class="onboarding__actions">
            <n-button @click="retryInit">返回重试</n-button>
          </div>
        </div>

        <!-- 成功 -->
        <div v-else-if="initResult?.success" class="onboarding__success-block">
          <p class="onboarding__success">✓ 初始化成功</p>
          <div v-if="initResult.importedAgents.length > 0" class="onboarding__imported">
            <div
              v-for="agent in initResult.importedAgents"
              :key="agent.agentId"
              class="onboarding__imported-item"
            >
              <span class="onboarding__imported-name">{{ agent.agentId }}</span>
              <n-tag size="small">{{ strategyText[agent.strategy] }}</n-tag>
            </div>
          </div>
          <div v-else class="onboarding__imported-empty">
            没有预置 agent 被导入
          </div>
          <div class="onboarding__actions">
            <n-button type="primary" @click="finish">进入应用</n-button>
          </div>
        </div>

        <!-- 兜底（不应到达） -->
        <div v-else class="onboarding__unknown">
          <p>状态未知，请重试</p>
          <n-button @click="retryInit">返回重试</n-button>
        </div>
      </n-card>
    </div>
  </div>
</template>

<style scoped>
.onboarding {
  height: 100%;
  display: flex;
  align-items: center;
  justify-content: center;
  background: var(--bg-app);
}
.onboarding__container {
  width: 600px;
  max-width: 90vw;
}
.onboarding__logo {
  width: 48px;
  height: 48px;
  border-radius: var(--radius-md);
  background: var(--brand-gradient);
  display: flex;
  align-items: center;
  justify-content: center;
  margin: 0 auto 16px;
  box-shadow: 0 4px 16px rgba(99, 102, 241, 0.3);
}
.onboarding__title {
  margin: 0 0 8px 0;
  font-size: 28px;
  font-weight: 800;
  text-align: center;
  letter-spacing: -0.5px;
  background: var(--brand-gradient);
  -webkit-background-clip: text;
  -webkit-text-fill-color: transparent;
  background-clip: text;
}
.onboarding__subtitle {
  margin: 0 0 28px 0;
  text-align: center;
  color: var(--text-tertiary);
  font-size: 14px;
}
.onboarding__steps { margin-bottom: 24px; }
.onboarding__card {
  margin-bottom: 16px;
  border-radius: var(--radius-lg);
  box-shadow: var(--shadow-md);
}
.onboarding__tip { margin-top: 12px; }
.onboarding__actions {
  display: flex;
  justify-content: flex-end;
  gap: var(--space-sm);
  margin-top: 16px;
}
.onboarding__step-desc {
  margin: 0 0 16px 0;
  color: var(--text-secondary);
  font-size: 14px;
}
.onboarding__presets { display: flex; flex-direction: column; gap: var(--space-sm); }
.onboarding__preset-item {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 12px 16px;
  border: 1px solid var(--border-light);
  border-radius: var(--radius-md);
  cursor: pointer;
  transition: var(--transition);
}
.onboarding__preset-item:hover { border-color: var(--brand-primary-light); }
.onboarding__preset-item--active {
  border-color: var(--brand-primary);
  background: rgba(99, 102, 241, 0.05);
}
.onboarding__preset-name { font-weight: 600; font-size: 14px; }
.onboarding__preset-path { color: var(--text-tertiary); font-size: 12px; margin-left: auto; }

.onboarding__strategy { margin-top: 20px; padding-top: 16px; border-top: 1px solid var(--border-light); }
.onboarding__strategy-title { font-weight: 600; font-size: 14px; margin-bottom: 4px; }
.onboarding__strategy-desc { font-size: 12px; color: var(--text-tertiary); margin-bottom: 12px; }
.onboarding__strategy-group { display: flex; flex-direction: column; gap: var(--space-sm); }
.onboarding__strategy-item {
  padding: 8px 12px;
  border: 1px solid var(--border-light);
  border-radius: var(--radius-sm);
}
.onboarding__strategy-item-desc { font-size: 12px; color: var(--text-tertiary); margin-top: 4px; padding-left: 24px; }

.onboarding__loading { text-align: center; padding: 40px 0; }
.onboarding__loading-text { margin-top: 16px; font-size: 15px; color: var(--text-secondary); }
.onboarding__loading-hint { margin-top: 4px; font-size: 12px; color: var(--text-tertiary); }
.onboarding__error-title { text-align: center; font-size: 18px; color: var(--color-error); margin: 16px 0; }
.onboarding__success { text-align: center; font-size: 18px; color: var(--color-success); margin: 16px 0; }
.onboarding__success-block { padding: 8px 0; }
.onboarding__imported { margin: 16px 0; }
.onboarding__imported-item {
  display: flex; justify-content: space-between; align-items: center;
  padding: 8px 12px; border-bottom: 1px solid var(--border-light);
}
.onboarding__imported-name { font-weight: 500; }
.onboarding__imported-empty { text-align: center; color: var(--text-tertiary); font-size: 13px; margin: 16px 0; }
.onboarding__unknown { text-align: center; padding: 20px; }
</style>
