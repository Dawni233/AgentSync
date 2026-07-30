<script setup lang="ts">
import type { SyncStatus } from '@/types'

defineProps<{
  status: SyncStatus
}>()

const map: Record<SyncStatus, { label: string; color: string; pulse: boolean }> = {
  idle: { label: '已同步', color: 'var(--success)', pulse: false },
  syncing: { label: '同步中', color: 'var(--brand)', pulse: true },
  pending: { label: '待同步', color: 'var(--warning)', pulse: false },
  conflict: { label: '冲突', color: 'var(--warning)', pulse: false },
  error: { label: '错误', color: 'var(--error)', pulse: false }
}
</script>

<template>
  <span class="sbadge">
    <span
      class="sbadge__d"
      :style="{ background: map[status].color }"
      :class="{ 'is-pulsing': map[status].pulse }"
    />
    <span>{{ map[status].label }}</span>
  </span>
</template>
