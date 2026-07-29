import { defineStore } from 'pinia'
import { ref } from 'vue'
import type { SyncResult, ConflictDetectedPayload } from '@/types'
import { useSync } from '@/composables/useSync'

/**
 * 同步状态 store
 * 接入后端 sync IPC + 事件监听
 */
export const useSyncStore = defineStore('sync', () => {
  const syncing = ref(false)
  const syncingAgentIds = ref<Set<string>>(new Set())
  const lastResults = ref<Map<string, SyncResult>>(new Map())
  const error = ref<string | null>(null)
  const pendingConflict = ref<ConflictDetectedPayload | null>(null)

  const { syncAgent: doSyncAgent, syncAll: doSyncAll, resolveConflict: doResolve } = useSync()

  async function syncAgent(agentId: string) {
    syncing.value = true
    syncingAgentIds.value.add(agentId)
    error.value = null
    try {
      const result = await doSyncAgent(agentId)
      lastResults.value.set(agentId, result)
      if (result.status === 'conflict') {
        // 冲突已通过 conflict:detected 事件处理
      }
      return result
    } catch (e) {
      error.value = String(e)
      throw e
    } finally {
      syncingAgentIds.value.delete(agentId)
      if (syncingAgentIds.value.size === 0) {
        syncing.value = false
      }
    }
  }

  async function syncAll() {
    syncing.value = true
    error.value = null
    try {
      const results = await doSyncAll()
      for (const r of results) {
        lastResults.value.set(r.agentId, r)
      }
      return results
    } catch (e) {
      error.value = String(e)
      throw e
    } finally {
      syncing.value = false
    }
  }

  async function resolveConflict(resolution: Parameters<typeof doResolve>[0]) {
    const result = await doResolve(resolution)
    lastResults.value.set(result.agentId, result)
    pendingConflict.value = null
    return result
  }

  function setPendingConflict(payload: ConflictDetectedPayload) {
    pendingConflict.value = payload
  }

  return {
    syncing,
    syncingAgentIds,
    lastResults,
    error,
    pendingConflict,
    syncAgent,
    syncAll,
    resolveConflict,
    setPendingConflict
  }
})
