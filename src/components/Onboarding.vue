<script setup lang="ts">
import { ref, reactive } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { showToast } from '@/composables/useToast'

const emit = defineEmits<{ completed: [] }>()

const toast = showToast

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
    toast('请填写仓库 URL 和 PAT')
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
    if (ok) toast('凭据验证通过')
    else toast('凭据验证失败')
  } catch (e) {
    toast(`验证失败: ${e}`)
  } finally {
    authTesting.value = false
  }
}

function goToStep2() {
  if (!authVerified.value) {
    toast('请先验证凭据')
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

function togglePreset(id: string) {
  if (selectedPresets.value.includes(id)) {
    selectedPresets.value = selectedPresets.value.filter((x) => x !== id)
  } else {
    selectedPresets.value.push(id)
  }
}

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
      toast('初始化完成')
      currentStep.value = 3
    } else {
      initError.value = result.errorMessage || '初始化失败'
      toast('初始化失败')
    }
  } catch (e) {
    initError.value = String(e)
    toast(`初始化失败: ${e}`)
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
  <div class="onb">
    <div class="onb__card">
      <div class="onb__logo">
        <svg width="22" height="22" viewBox="0 0 20 20" fill="none">
          <path d="M10 2L3 6v8l7 4 7-4V6l-7-4z" stroke="currentColor" stroke-width="1.5" stroke-linejoin="round" />
          <path d="M10 2v16M3 6l7 4 7-4M3 14l7-4 7 4" stroke="currentColor" stroke-width="1.5" stroke-linejoin="round" opacity="0.55" />
        </svg>
      </div>
      <h1 class="onb__title">AgentSync</h1>
      <p class="onb__sub">配置 Git 仓库以开始同步你的 AI 助手配置</p>

      <div class="steps">
        <div class="steps__dot" :class="{ 'is-done': currentStep > 1, 'is-active': currentStep === 1 }">
          <span class="steps__n">1</span><span class="steps__t">仓库凭据</span>
        </div>
        <div class="steps__line" />
        <div class="steps__dot" :class="{ 'is-done': currentStep > 2, 'is-active': currentStep === 2 }">
          <span class="steps__n">2</span><span class="steps__t">预置 Agent</span>
        </div>
        <div class="steps__line" />
        <div class="steps__dot" :class="{ 'is-active': currentStep === 3 }">
          <span class="steps__n">3</span><span class="steps__t">完成</span>
        </div>
      </div>

      <!-- 步骤 1：凭据 -->
      <div v-if="currentStep === 1">
        <div class="field" style="margin-bottom: 14px">
          <label class="field__label">仓库 URL</label>
          <input v-model="credentials.repoUrl" class="inp" placeholder="https://gitee.com/user/workbuddy-sync.git" />
        </div>
        <div class="field" style="margin-bottom: 14px">
          <label class="field__label">平台</label>
          <select v-model="credentials.platform" class="sel">
            <option v-for="o in platformOptions" :key="o.value" :value="o.value">{{ o.label }}</option>
          </select>
        </div>
        <div class="field" style="margin-bottom: 14px">
          <label class="field__label">访问令牌</label>
          <div class="form-row__ctrl" style="width: 100%">
            <input v-model="credentials.patToken" type="password" class="inp" placeholder="Personal Access Token" style="flex: 1" />
            <button class="btn btn--ghost" :disabled="authTesting" @click="verifyAuth">{{ authTesting ? '验证中…' : '验证' }}</button>
            <span v-if="authVerified" class="tag tag--ok">已验证</span>
          </div>
        </div>
        <div class="notice notice--info">请使用私有仓库同步。PAT 需有仓库读写权限。</div>
        <div class="onb__actions">
          <button class="btn btn--primary" :disabled="!authVerified" @click="goToStep2">下一步</button>
        </div>
      </div>

      <!-- 步骤 2：预置 agent + 导入策略 -->
      <div v-else-if="currentStep === 2">
        <p style="margin: 0 0 14px; color: var(--ink-2); font-size: 14px">
          选择要预置的 AI 助手。应用会检测本地配置目录并自动导入。
        </p>
        <div class="preset-grid">
          <label
            v-for="agent in presetAgents"
            :key="agent.id"
            class="preset"
            :class="{ 'is-on': selectedPresets.includes(agent.id) }"
            @click="togglePreset(agent.id)"
          >
            <span class="preset__check" />
            <span class="preset__name">{{ agent.label }}</span>
            <span class="preset__path">{{ agent.desc }}</span>
          </label>
        </div>

        <div style="margin-top: 22px">
          <div style="font-weight: 600; font-size: 14px; margin-bottom: 6px">导入策略</div>
          <div style="font-size: 12px; color: var(--ink-3); margin-bottom: 12px">
            当本地和远程都有配置时，如何处理冲突：
          </div>
          <div class="strat-grid">
            <div
              v-for="opt in strategyOptions"
              :key="opt.value"
              class="strat"
              :class="{ 'is-on': importStrategy === opt.value }"
              @click="importStrategy = opt.value as any"
            >
              <div class="strat__t">{{ opt.label }}</div>
              <div class="strat__d">{{ opt.desc }}</div>
            </div>
          </div>
        </div>

        <div class="onb__actions">
          <button class="btn btn--ghost" @click="currentStep = 1">上一步</button>
          <button class="btn btn--primary" :disabled="selectedPresets.length === 0" @click="startInit">开始初始化</button>
        </div>
      </div>

      <!-- 步骤 3：完成 / 错误 / 加载中 -->
      <div v-else>
        <div v-if="initializing" class="onb__loading">
          <div class="onb__spin" />
          <p style="margin-top: 16px; color: var(--ink-2)">正在初始化，请稍候…</p>
          <p style="font-size: 12px; color: var(--ink-3)">（clone 仓库 + 导入配置 + 推送到远程）</p>
        </div>
        <div v-else-if="initError" class="onb__error">
          <p style="color: var(--error); text-align: center; font-size: 18px; margin: 16px 0">✗ 初始化失败</p>
          <div class="notice notice--error">{{ initError }}</div>
          <div class="onb__actions">
            <button class="btn btn--ghost" @click="retryInit">返回重试</button>
          </div>
        </div>
        <div v-else-if="initResult?.success" class="onb__success-block">
          <p style="color: var(--success); text-align: center; font-size: 18px; margin: 16px 0">✓ 初始化成功</p>
          <div v-if="initResult.importedAgents.length > 0">
            <div v-for="agent in initResult.importedAgents" :key="agent.agentId" class="imported-row">
              <span style="font-weight: 500">{{ agent.agentId }}</span>
              <span class="tag">{{ strategyText[agent.strategy] }}</span>
            </div>
          </div>
          <div v-else style="text-align: center; color: var(--ink-3); margin: 16px 0">没有预置 agent 被导入</div>
          <div class="onb__actions">
            <button class="btn btn--primary" @click="finish">进入应用</button>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.onb__loading, .onb__error, .onb__success-block { text-align: center; padding: 24px 0; }
.onb__spin {
  width: 30px; height: 30px; margin: 0 auto;
  border: 3px solid var(--line-strong); border-top-color: var(--brand);
  border-radius: 50%; animation: onb-spin 0.7s linear infinite;
}
@keyframes onb-spin { to { transform: rotate(360deg); } }
</style>
