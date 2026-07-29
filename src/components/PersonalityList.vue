<script setup lang="ts">
import type { Persona } from '@/types'

defineProps<{
  personalities: Persona[]
  currentName?: string
}>()
</script>

<template>
  <!-- Phase 4 实现：人格列表、切换/编辑/删除按钮 -->
  <div class="personality-list">
    人格列表（Phase 4 实现）
  </div>
</template>

<style scoped>
.personality-list {
  padding: 16px;
  color: var(--n-text-color-3, #999);
  font-size: 13px;
}
</style>
