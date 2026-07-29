<script setup lang="ts">
import type { SyncStatus } from '@/types'

defineProps<{
  status: SyncStatus
}>()

const statusText: Record<SyncStatus, string> = {
  idle: '已同步',
  syncing: '同步中',
  pending: '待同步',
  conflict: '冲突',
  error: '错误'
}

const statusColor: Record<SyncStatus, string> = {
  idle: '#22c55e',
  syncing: '#3b82f6',
  pending: '#f59e0b',
  conflict: '#f59e0b',
  error: '#ef4444'
}
</script>

<template>
  <span class="sync-badge">
    <span class="sync-badge__dot" :style="{ background: statusColor[status] }" />
    <span class="sync-badge__text">{{ statusText[status] }}</span>
  </span>
</template>

<style scoped>
.sync-badge {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  font-size: 12px;
}
.sync-badge__dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  display: inline-block;
}
.sync-badge__text {
  color: var(--n-text-color-2, #666);
}
</style>
