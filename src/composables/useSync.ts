import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import type { SyncResult, ConflictDetectedPayload } from '@/types'

/**
 * 同步逻辑 composable
 * 封装 sync_agent / sync_all / resolve_conflict 的 IPC 调用 + 事件监听
 */
export function useSync() {
  /** 同步单个 agent */
  async function syncAgent(agentId: string): Promise<SyncResult> {
    return invoke<SyncResult>('sync_agent', { agentId })
  }

  /** 同步所有 agent */
  async function syncAll(): Promise<SyncResult[]> {
    return invoke<SyncResult[]>('sync_all')
  }

  /**
   * 解决冲突（前端弹窗用户决策后调用）
   * @param resolution 决策对象，type 字段对应后端 ConflictResolution 枚举
   */
  async function resolveConflict(resolution: {
    type:
      | 'L1ExportPatch'
      | 'L1DiscardLocal'
      | 'L1Cancel'
      | 'L2KeepLocal'
      | 'L2KeepRemote'
      | 'L2ManualMerge'
      | 'L2Cancel'
    mergedFiles?: string[]
  }): Promise<SyncResult> {
    return invoke<SyncResult>('resolve_conflict', { resolution })
  }

  return { syncAgent, syncAll, resolveConflict }
}

/**
 * 监听同步事件（在组件 onMounted 调用，onUnmounted 调用返回的 unlisten）
 *
 * 事件类型：
 * - sync:started -> { agentId }
 * - sync:completed -> { result: SyncResult }
 * - sync:error -> { agentId, errorMessage }
 * - conflict:detected -> ConflictDetectedPayload
 */
export function useSyncEvents(handlers: {
  onStarted?: (agentId: string) => void
  onCompleted?: (result: SyncResult) => void
  onError?: (agentId: string, errorMessage: string) => void
  onConflict?: (payload: ConflictDetectedPayload) => void
}): Promise<UnlistenFn> {
  return new Promise(async (resolve) => {
    const unlisteners: UnlistenFn[] = []

    if (handlers.onStarted) {
      unlisteners.push(
        await listen<{ agentId: string }>('sync:started', (e) => {
          handlers.onStarted!(e.payload.agentId)
        })
      )
    }
    if (handlers.onCompleted) {
      unlisteners.push(
        await listen<{ result: SyncResult }>('sync:completed', (e) => {
          handlers.onCompleted!(e.payload.result)
        })
      )
    }
    if (handlers.onError) {
      unlisteners.push(
        await listen<{ agentId: string; errorMessage: string }>('sync:error', (e) => {
          handlers.onError!(e.payload.agentId, e.payload.errorMessage)
        })
      )
    }
    if (handlers.onConflict) {
      unlisteners.push(
        await listen<ConflictDetectedPayload>('conflict:detected', (e) => {
          handlers.onConflict!(e.payload)
        })
      )
    }

    // 返回一个统一卸载函数
    resolve(() => {
      unlisteners.forEach((fn) => fn())
    })
  })
}
