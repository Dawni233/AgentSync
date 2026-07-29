import { defineStore } from 'pinia'
import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import type { Settings } from '@/types'

/**
 * 应用设置 store
 * 接入后端 invoke('get_settings') / invoke('save_settings')
 */
export const useSettingsStore = defineStore('settings', () => {
  const settings = ref<Settings | null>(null)
  const loading = ref(false)

  async function loadSettings() {
    loading.value = true
    try {
      settings.value = await invoke<Settings>('get_settings')
    } finally {
      loading.value = false
    }
  }

  async function saveSettings(newSettings: Settings) {
    await invoke('save_settings', { settings: newSettings })
    settings.value = newSettings
  }

  return { settings, loading, loadSettings, saveSettings }
})
