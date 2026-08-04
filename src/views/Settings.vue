<script setup lang="ts">
import { ref, onMounted, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { open as openDialog } from '@tauri-apps/plugin-dialog'
import { useSettingsStore } from '@/stores/settings'
import { showToast } from '@/composables/useToast'
import type { Agent, AgentConfig, Settings } from '@/types'

const settingsStore = useSettingsStore()
const toast = showToast

const form = ref<Settings>({
  repoUrl: '',
  platform: 'gitee',
  patToken: '',
  autoSyncEnabled: false,
  autoSyncIntervalMin: 15,
  launchAtLogin: false
})

const authTesting = ref(false)
const authResult = ref<boolean | null>(null)

const platformOptions = [
  { label: 'Gitee', value: 'gitee' },
  { label: 'GitHub', value: 'github' }
]

const intervalOptions = [
  { label: '5 分钟', value: 5 },
  { label: '15 分钟', value: 15 },
  { label: '30 分钟', value: 30 },
  { label: '60 分钟', value: 60 }
]

async function testGitAuth() {
  if (!form.value.repoUrl || !form.value.patToken) {
    toast('请先填写仓库 URL 和 PAT')
    return
  }
  authTesting.value = true
  authResult.value = null
  try {
    authResult.value = await invoke<boolean>('test_git_auth', {
      url: form.value.repoUrl,
      token: form.value.patToken
    })
    toast(authResult.value ? '凭据验证通过' : '凭据验证失败')
  } catch (e) {
    authResult.value = false
    toast(`验证失败: ${e}`)
  } finally {
    authTesting.value = false
  }
}

async function saveSettings() {
  await settingsStore.saveSettings(form.value)
  let hasWarning = false
  try {
    if (form.value.autoSyncEnabled) await invoke('start_auto_sync')
    else await invoke('stop_auto_sync')
  } catch (e) {
    toast(`自动同步控制失败: ${e}`)
    hasWarning = true
  }
  try {
    await invoke('set_autostart', { enabled: form.value.launchAtLogin })
  } catch (e) {
    toast(`开机自启设置失败: ${e}`)
    hasWarning = true
  }
  if (!hasWarning) toast('设置已保存')
}

// 已注册 Agent 列表
const agentList = ref<Agent[]>([])
const showAddAgent = ref(false)
const newAgent = ref<AgentConfig>({
  id: '',
  displayName: '',
  configDir: '',
  syncFiles: [],
  excludeFiles: []
})
const newAgentSyncFiles = ref('')
const newAgentExcludeFiles = ref('')

const presetTemplates = [
  { id: 'workbuddy', displayName: 'WorkBuddy', configDir: '~/.workbuddy', syncFiles: ['SOUL.md', 'IDENTITY.md', 'USER.md', 'MEMORY.md', 'memory/**'], excludeFiles: ['memory/cache/', 'memory/tmp/', '*.lock', '*.tmp', '*.log', '*.bak'] },
  { id: 'claude-code', displayName: 'Claude Code', configDir: '~/.claude', syncFiles: ['CLAUDE.md', 'settings.json'], excludeFiles: ['*.log'] },
  { id: 'cursor', displayName: 'Cursor', configDir: '~/.cursor', syncFiles: ['rules/**'], excludeFiles: [] },
  { id: 'codex', displayName: 'Codex', configDir: '~/.codex', syncFiles: ['config.toml'], excludeFiles: ['*.sqlite', '*.sqlite-shm', '*.sqlite-wal', 'logs_*.sqlite', 'installation_id', 'tmp/', '.tmp/', 'sqlite/', 'vendor_imports/', 'skills/'] },
  { id: 'zcode', displayName: 'ZCode', configDir: '~/.zcode', syncFiles: ['AGENTS.md'], excludeFiles: ['cli/', 'plugin-workspace/', 'v2/', 'agents/', 'skills/'] },
  { id: 'qoder', displayName: 'Qoder', configDir: '~/.qoderworkcn', syncFiles: ['commands/**'], excludeFiles: ['bin/', 'cache/', 'logs/', 'machine-id', 'skills/'] },
  { id: 'openclaw', displayName: 'OpenClaw', configDir: '~/.openclaw', syncFiles: ['identity/**'], excludeFiles: ['exec-approvals.json', '*.sock'] },
  { id: 'qwenpaw', displayName: 'QwenPaw', configDir: '~/.qwenpaw', syncFiles: ['HEARTBEAT.md', 'config.json', 'settings.json'], excludeFiles: ['qwenpaw.log', 'token_usage.json', 'workspaces/', 'skill_pool/'] },
  { id: 'custom', displayName: '自定义', configDir: '', syncFiles: [], excludeFiles: [] }
]
const selectedTemplate = ref('custom')

const templateOptions = presetTemplates.map((t) => ({
  label: t.displayName + (t.id === 'custom' ? '' : ` (${t.configDir})`),
  value: t.id
}))

watch(selectedTemplate, (val) => {
  const tpl = presetTemplates.find((t) => t.id === val)
  if (!tpl || tpl.id === 'custom') return
  newAgent.value.id = tpl.id
  newAgent.value.displayName = tpl.displayName
  newAgent.value.configDir = tpl.configDir
  newAgentSyncFiles.value = tpl.syncFiles.join('\n')
  newAgentExcludeFiles.value = tpl.excludeFiles.join('\n')
})

async function browseConfigDir() {
  const selected = await openDialog({
    title: '选择 AI 助手配置目录',
    directory: true,
    multiple: false
  })
  if (selected && !Array.isArray(selected)) {
    newAgent.value.configDir = selected
  }
}

async function loadAgents() {
  try {
    agentList.value = await invoke<Agent[]>('get_agents')
  } catch (e) {
    console.warn('加载 agents 失败:', e)
  }
}

function openAddAgentDialog() {
  newAgent.value = { id: '', displayName: '', configDir: '', syncFiles: [], excludeFiles: [] }
  newAgentSyncFiles.value = ''
  newAgentExcludeFiles.value = ''
  selectedTemplate.value = 'custom'
  showAddAgent.value = true
}

const adding = ref(false)

async function confirmAddAgent() {
  if (!newAgent.value.id || !newAgent.value.displayName || !newAgent.value.configDir) {
    toast('请填写 ID、显示名、配置目录')
    return
  }
  adding.value = true
  newAgent.value.syncFiles = newAgentSyncFiles.value
    .split('\n')
    .map((s) => s.trim())
    .filter((s) => s.length > 0)
  newAgent.value.excludeFiles = newAgentExcludeFiles.value
    .split('\n')
    .map((s) => s.trim())
    .filter((s) => s.length > 0)
  try {
    await invoke('add_agent', { config: newAgent.value })
    toast(`已添加 ${newAgent.value.displayName}`)
    showAddAgent.value = false
    newAgent.value = { id: '', displayName: '', configDir: '', syncFiles: [], excludeFiles: [] }
    newAgentSyncFiles.value = ''
    newAgentExcludeFiles.value = ''
    await loadAgents()
  } catch (e) {
    toast(`添加失败: ${e}`)
  } finally {
    adding.value = false
  }
}

async function onRemoveAgent(agentId: string) {
  try {
    await invoke('remove_agent', { agentId })
    toast(`已删除 ${agentId}`)
    await loadAgents()
  } catch (e) {
    toast(`删除失败: ${e}`)
  }
}

onMounted(async () => {
  try {
    await settingsStore.loadSettings()
    if (settingsStore.settings) {
      form.value = { ...settingsStore.settings }
    }
  } catch (e) {
    console.warn('加载设置失败:', e)
  }
  await loadAgents()
})
</script>

<template>
  <div class="settings-view">
    <h2 class="settings-view__title">设置</h2>

    <!-- Git 仓库配置 -->
    <div class="section">
      <h3 class="section__title">Git 仓库配置</h3>
      <div class="form-row">
        <div class="form-row__label">仓库 URL</div>
        <div class="form-row__ctrl">
          <input v-model="form.repoUrl" class="inp" placeholder="https://gitee.com/user/workbuddy-sync.git" />
        </div>
      </div>
      <div class="form-row">
        <div class="form-row__label">平台</div>
        <div class="form-row__ctrl">
          <select v-model="form.platform" class="sel">
            <option v-for="o in platformOptions" :key="o.value" :value="o.value">{{ o.label }}</option>
          </select>
        </div>
      </div>
      <div class="form-row">
        <div class="form-row__label">访问令牌</div>
        <div class="form-row__ctrl">
          <input v-model="form.patToken" type="password" class="inp" style="flex: 1" placeholder="Personal Access Token" />
          <button class="btn btn--ghost" :disabled="authTesting" @click="testGitAuth">{{ authTesting ? '验证中…' : '验证' }}</button>
          <span v-if="authResult === true" class="tag tag--ok">通过</span>
          <span v-else-if="authResult === false" class="tag">失败</span>
        </div>
      </div>
    </div>

    <!-- 自动同步 -->
    <div class="section">
      <h3 class="section__title">自动同步</h3>
      <div class="form-row">
        <div class="form-row__label">启用自动同步</div>
        <div class="form-row__ctrl">
          <div
            class="switch"
            :class="{ 'is-on': form.autoSyncEnabled }"
            role="switch"
            tabindex="0"
            aria-label="启用自动同步"
            @click="form.autoSyncEnabled = !form.autoSyncEnabled"
            @keydown.space.prevent="form.autoSyncEnabled = !form.autoSyncEnabled"
          />
        </div>
      </div>
      <div class="form-row">
        <div class="form-row__label">同步间隔</div>
        <div class="form-row__ctrl">
          <select v-model.number="form.autoSyncIntervalMin" class="sel" :disabled="!form.autoSyncEnabled">
            <option v-for="o in intervalOptions" :key="o.value" :value="o.value">{{ o.label }}</option>
          </select>
        </div>
      </div>
      <div class="form-row">
        <div class="form-row__label">开机自启动</div>
        <div class="form-row__ctrl">
          <div
            class="switch"
            :class="{ 'is-on': form.launchAtLogin }"
            role="switch"
            tabindex="0"
            aria-label="开机自启动"
            @click="form.launchAtLogin = !form.launchAtLogin"
            @keydown.space.prevent="form.launchAtLogin = !form.launchAtLogin"
          />
        </div>
      </div>
    </div>

    <!-- 已注册 Agent -->
    <div class="section">
      <h3 class="section__title">已注册 Agent</h3>
      <table class="dtable">
        <thead>
          <tr><th>ID</th><th>显示名</th><th>配置目录</th><th>操作</th></tr>
        </thead>
        <tbody>
          <tr v-for="a in agentList" :key="a.id">
            <td>{{ a.id }}</td>
            <td>{{ a.displayName }}</td>
            <td>{{ a.configDir }}</td>
            <td><button class="btn btn--danger btn--quiet" @click="onRemoveAgent(a.id)">删除</button></td>
          </tr>
          <tr v-if="agentList.length === 0">
            <td colspan="4" style="text-align: center; color: var(--ink-3)">暂无已注册 Agent</td>
          </tr>
        </tbody>
      </table>
      <div style="margin-top: 12px">
        <button class="btn btn--ghost" @click="openAddAgentDialog">添加 Agent</button>
      </div>
    </div>

    <!-- 添加 Agent 弹窗 -->
    <div v-if="showAddAgent" class="modal-mask" @click.self="showAddAgent = false">
      <div class="modal">
        <div class="modal__head"><h3 class="modal__title">添加 Agent</h3></div>
        <div class="modal__body">
          <div class="form-row">
            <div class="form-row__label">预置模板</div>
            <div class="form-row__ctrl">
              <select v-model="selectedTemplate" class="sel">
                <option v-for="o in templateOptions" :key="o.value" :value="o.value">{{ o.label }}</option>
              </select>
            </div>
          </div>
          <div class="form-row">
            <div class="form-row__label">ID</div>
            <div class="form-row__ctrl"><input v-model="newAgent.id" class="inp" placeholder="如 my-agent（小写）" /></div>
          </div>
          <div class="form-row">
            <div class="form-row__label">显示名</div>
            <div class="form-row__ctrl"><input v-model="newAgent.displayName" class="inp" placeholder="如 My Agent" /></div>
          </div>
          <div class="form-row">
            <div class="form-row__label">配置目录</div>
            <div class="form-row__ctrl">
              <input v-model="newAgent.configDir" class="inp" style="flex: 1" placeholder="如 ~/.myagent" />
              <button class="btn btn--ghost" @click="browseConfigDir">浏览</button>
            </div>
          </div>
          <div class="form-row">
            <div class="form-row__label">同步文件<small>每行一个 glob</small></div>
            <div class="form-row__ctrl"><textarea v-model="newAgentSyncFiles" class="ta" rows="4" placeholder="SOUL.md" style="flex: 1; min-width: 0" /></div>
          </div>
          <div class="form-row">
            <div class="form-row__label">排除文件<small>可留空</small></div>
            <div class="form-row__ctrl"><textarea v-model="newAgentExcludeFiles" class="ta" rows="3" placeholder="*.log" style="flex: 1; min-width: 0" /></div>
          </div>
        </div>
        <div class="modal__foot">
          <button class="btn btn--ghost" @click="showAddAgent = false">取消</button>
          <button class="btn btn--primary" :disabled="adding" @click="confirmAddAgent">{{ adding ? '添加中…' : '添加' }}</button>
        </div>
      </div>
    </div>

    <div class="form-actions">
      <button class="btn btn--primary" @click="saveSettings">保存设置</button>
    </div>
  </div>
</template>
