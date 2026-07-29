import { defineStore } from 'pinia'
import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import type { Agent, AgentConfig } from '@/types'

/**
 * Agent 管理 store
 * 接入后端 invoke('get_agents') / invoke('add_agent') / invoke('remove_agent')
 */
export const useAgentsStore = defineStore('agents', () => {
  const agents = ref<Agent[]>([])
  const loading = ref(false)

  async function loadAgents() {
    loading.value = true
    try {
      agents.value = await invoke<Agent[]>('get_agents')
    } finally {
      loading.value = false
    }
  }

  async function addAgent(config: AgentConfig) {
    await invoke('add_agent', { config })
    agents.value.push({
      ...config,
      currentPersona: null,
      syncStatus: 'idle',
      lastSyncAt: null,
      trackedFileCount: 0
    })
  }

  async function removeAgent(agentId: string) {
    await invoke('remove_agent', { agentId })
    agents.value = agents.value.filter((a) => a.id !== agentId)
  }

  return { agents, loading, loadAgents, addAgent, removeAgent }
})
