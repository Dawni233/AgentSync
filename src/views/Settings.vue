<script setup lang="ts">
import { ref, onMounted, h, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { open as openDialog } from '@tauri-apps/plugin-dialog'
import {
  NCard,
  NForm,
  NFormItem,
  NInput,
  NSelect,
  NButton,
  NSwitch,
  NSpace,
  NTag,
  NDataTable,
  NModal,
  useMessage
} from 'naive-ui'
import type { DataTableColumns } from 'naive-ui'
import type { Agent, AgentConfig } from '@/types'
import { useSettingsStore } from '@/stores/settings'
import type { Settings } from '@/types'

const settingsStore = useSettingsStore()
const message = useMessage()

// 表单本地副本，保存后才写入 store
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
    message.warning('请先填写仓库 URL 和 PAT')
    return
  }
  authTesting.value = true
  authResult.value = null
  try {
    authResult.value = await invoke<boolean>('test_git_auth', {
      url: form.value.repoUrl,
      token: form.value.patToken
    })
    if (authResult.value) {
      message.success('凭据验证通过')
    } else {
      message.error('凭据验证失败')
    }
  } catch (e) {
    authResult.value = false
    message.error(`验证失败: ${e}`)
  } finally {
    authTesting.value = false
  }
}

async function saveSettings() {
  await settingsStore.saveSettings(form.value)
  let hasWarning = false
  // 同步控制自动同步定时器
  try {
    if (form.value.autoSyncEnabled) {
      await invoke('start_auto_sync')
    } else {
      await invoke('stop_auto_sync')
    }
  } catch (e) {
    message.warning(`自动同步控制失败: ${e}`)
    hasWarning = true
  }
  // 同步控制开机自启
  try {
    await invoke('set_autostart', { enabled: form.value.launchAtLogin })
  } catch (e) {
    message.warning(`开机自启设置失败: ${e}`)
    hasWarning = true
  }
  if (!hasWarning) {
    message.success('设置已保存')
  }
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

const agentColumns: DataTableColumns<Agent> = [
  { title: 'ID', key: 'id', width: 120 },
  { title: '显示名', key: 'displayName', width: 120 },
  { title: '配置目录', key: 'configDir' },
  {
    title: '操作',
    key: 'actions',
    width: 80,
    render(row) {
      return h(
        NButton,
        { size: 'tiny', type: 'error', ghost: true, onClick: () => onRemoveAgent(row.id) },
        { default: () => '删除' }
      )
    }
  }
]

// 预置模板（与后端 registry.rs default_presets 一致）
const presetTemplates = [
  { id: 'workbuddy', displayName: 'WorkBuddy', configDir: '~/.workbuddy', syncFiles: ['SOUL.md', 'IDENTITY.md', 'USER.md', 'memory/**'], excludeFiles: ['memory/cache/', 'memory/tmp/', '*.lock', '*.tmp', '*.log', '*.bak'] },
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

// 选择模板时自动填充
watch(selectedTemplate, (val) => {
  const tpl = presetTemplates.find((t) => t.id === val)
  if (!tpl || tpl.id === 'custom') return
  newAgent.value.id = tpl.id
  newAgent.value.displayName = tpl.displayName
  newAgent.value.configDir = tpl.configDir
  newAgentSyncFiles.value = tpl.syncFiles.join('\n')
  newAgentExcludeFiles.value = tpl.excludeFiles.join('\n')
})

// 文件夹选择器
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
  // 重置表单
  newAgent.value = { id: '', displayName: '', configDir: '', syncFiles: [], excludeFiles: [] }
  newAgentSyncFiles.value = ''
  newAgentExcludeFiles.value = ''
  selectedTemplate.value = 'custom'
  showAddAgent.value = true
}

const adding = ref(false)

async function confirmAddAgent() {
  if (!newAgent.value.id || !newAgent.value.displayName || !newAgent.value.configDir) {
    message.warning('请填写 ID、显示名、配置目录')
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
    message.success(`已添加 ${newAgent.value.displayName}`)
    showAddAgent.value = false
    newAgent.value = { id: '', displayName: '', configDir: '', syncFiles: [], excludeFiles: [] }
    newAgentSyncFiles.value = ''
    newAgentExcludeFiles.value = ''
    await loadAgents()
  } catch (e) {
    message.error(`添加失败: ${e}`)
  } finally {
    adding.value = false
  }
}

async function onRemoveAgent(agentId: string) {
  try {
    await invoke('remove_agent', { agentId })
    message.success(`已删除 ${agentId}`)
    await loadAgents()
  } catch (e) {
    message.error(`删除失败: ${e}`)
  }
}

// 加载已有设置到表单
onMounted(async () => {
  try {
    await settingsStore.loadSettings()
    if (settingsStore.settings) {
      form.value = { ...settingsStore.settings }
    }
  } catch (e) {
    console.warn('加载设置失败:', e)
  }
  // 加载 agent 列表
  await loadAgents()
})
</script>

<template>
  <div class="settings">
    <h2 class="settings__title">设置</h2>

    <!-- Git 仓库配置 -->
    <n-card title="Git 仓库配置" size="small" class="settings__section">
      <n-form label-placement="left" :label-width="100">
        <n-form-item label="仓库 URL">
          <n-input
            v-model:value="form.repoUrl"
            placeholder="https://gitee.com/user/workbuddy-sync.git"
          />
        </n-form-item>
        <n-form-item label="平台">
          <n-select v-model:value="form.platform" :options="platformOptions" />
        </n-form-item>
        <n-form-item label="访问令牌">
          <n-space align="center">
            <n-input
              v-model:value="form.patToken"
              type="password"
              show-password-on="click"
              placeholder="Personal Access Token"
              style="width: 360px"
            />
            <n-button :loading="authTesting" @click="testGitAuth">验证</n-button>
            <n-tag v-if="authResult === true" type="success">通过</n-tag>
            <n-tag v-else-if="authResult === false" type="error">失败</n-tag>
          </n-space>
        </n-form-item>
      </n-form>
    </n-card>

    <!-- 自动同步 -->
    <n-card title="自动同步" size="small" class="settings__section">
      <n-form label-placement="left" :label-width="100">
        <n-form-item label="启用自动同步">
          <n-switch v-model:value="form.autoSyncEnabled" />
        </n-form-item>
        <n-form-item label="同步间隔">
          <n-select
            v-model:value="form.autoSyncIntervalMin"
            :options="intervalOptions"
            :disabled="!form.autoSyncEnabled"
            style="width: 200px"
          />
        </n-form-item>
        <n-form-item label="开机自启动">
          <n-switch v-model:value="form.launchAtLogin" />
        </n-form-item>
      </n-form>
    </n-card>

    <!-- 已注册 Agent -->
    <n-card title="已注册 Agent" size="small" class="settings__section">
      <n-data-table
        :columns="agentColumns"
        :data="agentList"
        :bordered="false"
        size="small"
      />
      <div class="settings__add-agent">
        <n-button size="small" @click="openAddAgentDialog">添加 Agent</n-button>
      </div>
    </n-card>

    <!-- 添加 Agent 弹窗 -->
    <n-modal
      v-model:show="showAddAgent"
      preset="card"
      title="添加 Agent"
      style="width: 560px"
    >
      <n-form label-placement="left" :label-width="100">
        <n-form-item label="预置模板">
          <n-select
            v-model:value="selectedTemplate"
            :options="templateOptions"
            placeholder="选择模板自动填充，或选自定义"
          />
        </n-form-item>
        <n-form-item label="ID">
          <n-input v-model:value="newAgent.id" placeholder="如 my-agent（小写）" />
        </n-form-item>
        <n-form-item label="显示名">
          <n-input v-model:value="newAgent.displayName" placeholder="如 My Agent" />
        </n-form-item>
        <n-form-item label="配置目录">
          <n-space align="center" :wrap="false" style="width: 100%">
            <n-input
              v-model:value="newAgent.configDir"
              placeholder="如 ~/.myagent"
              style="flex: 1"
            />
            <n-button @click="browseConfigDir">浏览</n-button>
          </n-space>
        </n-form-item>
        <n-form-item label="同步文件">
          <n-input
            v-model:value="newAgentSyncFiles"
            type="textarea"
            placeholder="每行一个 glob 模式，如 SOUL.md"
            :rows="4"
          />
        </n-form-item>
        <n-form-item label="排除文件">
          <n-input
            v-model:value="newAgentExcludeFiles"
            type="textarea"
            placeholder="每行一个 glob 模式，如 *.log（可留空）"
            :rows="3"
          />
        </n-form-item>
      </n-form>
      <template #footer>
        <n-space justify="end">
          <n-button @click="showAddAgent = false">取消</n-button>
          <n-button type="primary" :loading="adding" :disabled="adding" @click="confirmAddAgent">
            {{ adding ? '添加中...' : '添加' }}
          </n-button>
        </n-space>
      </template>
    </n-modal>

    <div class="settings__actions">
      <n-button type="primary" @click="saveSettings">保存设置</n-button>
    </div>
  </div>
</template>

<style scoped>
.settings {
  height: 100%;
  overflow-y: auto;
  padding: 16px 20px;
  background: #fff;
}
.settings__title {
  margin: 0 0 16px 0;
}
.settings__section {
  margin-bottom: 16px;
}
.settings__add-agent {
  margin-top: 12px;
}
.settings__actions {
  margin-top: 16px;
  padding-bottom: 16px;
}
</style>
