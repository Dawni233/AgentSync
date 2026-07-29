<script setup lang="ts">
defineProps<{
  filePath: string
}>()
</script>

<template>
  <!-- Phase 4 实现：文件预览面板、暗色代码编辑器 -->
  <div class="file-preview">
    文件预览（Phase 4 实现）
  </div>
</template>

<style scoped>
.file-preview {
  padding: 16px;
  color: var(--n-text-color-3, #999);
  font-size: 13px;
}
</style>
