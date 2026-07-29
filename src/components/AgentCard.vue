<script setup lang="ts">
import type { Agent } from '@/types'

defineProps<{
  agent: Agent
}>()
</script>

<template>
  <!-- Phase 2+ 实现：Agent 状态卡片 -->
  <div class="agent-card">
    Agent 卡片（Phase 2+ 实现）
  </div>
</template>

<style scoped>
.agent-card {
  padding: 16px;
  color: var(--n-text-color-3, #999);
  font-size: 13px;
}
</style>
