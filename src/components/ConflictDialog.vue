<script setup lang="ts">
import { computed } from 'vue'
import {
  NModal,
  NButton,
  NSpace,
  NAlert,
  NList,
  NListItem,
  NThing,
  NTag
} from 'naive-ui'
import type { ConflictDetectedPayload } from '@/types'

const props = defineProps<{
  show: boolean
  payload: ConflictDetectedPayload | null
  resolving: boolean
}>()

const emit = defineEmits<{
  resolve: [resolution: ResolutionOption]
  cancel: []
}>()

type ResolutionOption =
  | 'L1ExportPatch'
  | 'L1DiscardLocal'
  | 'L1Cancel'
  | 'L2KeepLocal'
  | 'L2KeepRemote'
  | 'L2ManualMerge'
  | 'L2Cancel'

const isL1 = computed(() => props.payload?.conflictType === 'L1')

const title = computed(() =>
  isL1.value ? 'Git 历史冲突（L1）' : '应用配置冲突（L2）'
)

const description = computed(() => {
  if (!props.payload) return ''
  if (isL1.value) {
    return '本地未推送的 commit 与远程 commit 修改了同一文件。请选择处理方式：'
  }
  return '本地配置目录与远程 _current/ 同时有变更。请选择保留哪一方：'
})

function resolve(option: ResolutionOption) {
  emit('resolve', option)
}

function cancel() {
  emit('cancel')
}
</script>

<template>
  <n-modal
    :show="show"
    :mask-closable="false"
    :close-on-esc="false"
    preset="card"
    :title="title"
    style="width: 600px; max-width: 90vw"
  >
    <div class="conflict-dialog">
      <n-alert :type="isL1 ? 'warning' : 'error'" :bordered="false">
        {{ description }}
      </n-alert>

      <!-- 冲突文件列表 -->
      <div v-if="payload?.files.length" class="conflict-dialog__files">
        <div class="conflict-dialog__files-title">冲突文件（{{ payload.files.length }}）：</div>
        <n-list bordered size="small">
          <n-list-item v-for="file in payload.files" :key="file.path">
            <n-thing>
              <template #header>
                <span class="conflict-dialog__file-path">{{ file.path }}</span>
              </template>
              <template #description>
                <n-space size="small">
                  <n-tag size="small" type="info">
                    本地: {{ new Date(file.localMtime).toLocaleString() }}
                  </n-tag>
                  <n-tag size="small" type="success">
                    远程: {{ new Date(file.remoteMtime).toLocaleString() }}
                  </n-tag>
                </n-space>
              </template>
            </n-thing>
          </n-list-item>
        </n-list>
      </div>

      <!-- L1 冲突选项 -->
      <div v-if="isL1" class="conflict-dialog__options">
        <div class="conflict-dialog__option" @click="resolve('L1ExportPatch')">
          <div class="conflict-dialog__option-title">导出 patch 文件</div>
          <div class="conflict-dialog__option-desc">
            将本地未推送的 commit 导出为 .patch 文件，供人工解决后重放
          </div>
        </div>
        <div class="conflict-dialog__option" @click="resolve('L1DiscardLocal')">
          <div class="conflict-dialog__option-title">放弃本地未推送 commit</div>
          <div class="conflict-dialog__option-desc">
            丢弃本地未推送的变更，使用远程版本继续同步
          </div>
        </div>
      </div>

      <!-- L2 冲突选项 -->
      <div v-else class="conflict-dialog__options">
        <div class="conflict-dialog__option" @click="resolve('L2KeepLocal')">
          <div class="conflict-dialog__option-title">保留本地版本</div>
          <div class="conflict-dialog__option-desc">
            用本地配置覆盖远程 _current/，本地变更优先
          </div>
        </div>
        <div class="conflict-dialog__option" @click="resolve('L2KeepRemote')">
          <div class="conflict-dialog__option-title">保留远程版本</div>
          <div class="conflict-dialog__option-desc">
            用远程 _current/ 覆盖本地配置目录
          </div>
        </div>
        <div class="conflict-dialog__option" @click="resolve('L2ManualMerge')">
          <div class="conflict-dialog__option-title">手动合并</div>
          <div class="conflict-dialog__option-desc">
            打开内置编辑器，逐文件合并（Phase 4 实现）
          </div>
        </div>
      </div>

      <div class="conflict-dialog__actions">
        <n-button :loading="resolving" @click="cancel">取消同步</n-button>
      </div>
    </div>
  </n-modal>
</template>

<style scoped>
.conflict-dialog {
  display: flex;
  flex-direction: column;
  gap: 16px;
}
.conflict-dialog__files {
  margin-top: 8px;
}
.conflict-dialog__files-title {
  font-size: 13px;
  color: #52525b;
  margin-bottom: 8px;
}
.conflict-dialog__file-path {
  font-family: 'Consolas', 'Monaco', monospace;
  font-size: 13px;
}
.conflict-dialog__options {
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.conflict-dialog__option {
  padding: 12px 16px;
  border: 1px solid #e4e4e7;
  border-radius: 6px;
  cursor: pointer;
  transition: all 0.15s;
}
.conflict-dialog__option:hover {
  border-color: #3b82f6;
  background: #eff6ff;
}
.conflict-dialog__option-title {
  font-weight: 600;
  font-size: 14px;
  margin-bottom: 4px;
}
.conflict-dialog__option-desc {
  font-size: 12px;
  color: #71717a;
}
.conflict-dialog__actions {
  display: flex;
  justify-content: flex-end;
  margin-top: 8px;
}
</style>
