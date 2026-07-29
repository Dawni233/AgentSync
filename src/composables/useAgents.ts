import type { Agent, AgentConfig } from '@/types'

/**
 * Agent 管理 composable
 * 封装 get_agents / add_agent / remove_agent 的 IPC 调用
 * Phase 2+ 后端实现后接入
 */
export function useAgents() {
  async function getAgents() {
    // TODO Phase 2: return await invoke<Agent[]>('get_agents')
    return [] as Agent[]
  }

  async function addAgent(config: AgentConfig) {
    // TODO Phase 2: await invoke('add_agent', { config })
    console.log('addAgent (stub):', config)
  }

  async function removeAgent(agentId: string) {
    // TODO Phase 2: await invoke('remove_agent', { agentId })
    console.log('removeAgent (stub):', agentId)
  }

  return { getAgents, addAgent, removeAgent }
}
