import { invoke } from '@tauri-apps/api/core'
import { open as openDialog, save as saveDialog } from '@tauri-apps/plugin-dialog'
import type { Persona } from '@/types'

/** 导入预览结果（与后端 PersonaDiffPreview 对应） */
export interface PersonaDiffPreview {
  name: string
  displayName: string
  files: Array<{ path: string; action: string }>
}

/**
 * 人格管理 composable
 * 封装 save/switch/delete/export/import 的 IPC 调用
 */
export function usePersonalities() {
  async function listPersonalities(agentId: string): Promise<Persona[]> {
    return invoke<Persona[]>('list_personalities', { agentId })
  }

  async function savePersonality(agentId: string, name: string): Promise<void> {
    await invoke('save_personality', { agentId, name })
  }

  async function switchPersonality(agentId: string, name: string): Promise<void> {
    await invoke('switch_personality', { agentId, name })
  }

  async function deletePersonality(agentId: string, name: string): Promise<void> {
    await invoke('delete_personality', { agentId, name })
  }

  /** 导出人格包：弹出保存对话框，返回保存路径 */
  async function exportPersonalities(agentId: string, names: string[]): Promise<string | null> {
    const filePath = await saveDialog({
      title: '导出人格包',
      defaultPath: `${agentId}-personas.zip`,
      filters: [{ name: 'Zip', extensions: ['zip'] }]
    })
    if (!filePath) return null
    const result = await invoke<string>('export_personalities', {
      agentId,
      names,
      outputPath: filePath
    })
    return result
  }

  /** 导入预览：弹出打开对话框，返回预览结果和 zip 路径 */
  async function previewImport(
    agentId: string
  ): Promise<{ previews: PersonaDiffPreview[]; zipPath: string } | null> {
    const filePath = await openDialog({
      title: '选择人格包',
      filters: [{ name: 'Zip', extensions: ['zip'] }],
      multiple: false
    })
    if (!filePath || Array.isArray(filePath)) return null
    const previews = await invoke<PersonaDiffPreview[]>('preview_import_personalities', {
      zipPath: filePath,
      agentId
    })
    return { previews, zipPath: filePath }
  }

  /** 确认导入：用户确认后实际解压 */
  async function importPersonalities(zipPath: string, agentId: string): Promise<void> {
    await invoke('import_personalities', { zipPath, agentId })
  }

  return {
    listPersonalities,
    savePersonality,
    switchPersonality,
    deletePersonality,
    exportPersonalities,
    previewImport,
    importPersonalities
  }
}
