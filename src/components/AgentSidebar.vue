<script setup lang="ts">
import type { Agent } from '@/types'

defineProps<{
  agents: Agent[]
  activeAgentId?: string
}>()

defineEmits<{
  select: [agentId: string]
}>()
</script>

<template>
  <!-- Phase 2+ 实现：Agent 列表、状态灯、选中态 -->
  <div class="agent-sidebar">
    <div class="agent-sidebar__placeholder">
      Agent 侧边栏（Phase 2+ 实现）
    </div>
  </div>
</template>

<style scoped>
.agent-sidebar {
  height: 100%;
}
.agent-sidebar__placeholder {
  padding: 16px;
  color: var(--n-text-color-3, #999);
  font-size: 13px;
}
</style>
