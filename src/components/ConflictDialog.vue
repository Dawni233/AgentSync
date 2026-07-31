<script setup lang="ts">
import { computed } from 'vue'
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

export type ResolutionOption =
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
  <div v-if="show" class="modal-mask">
    <div class="modal" role="dialog" aria-modal="true" aria-label="冲突解决">
      <div class="modal__head">
        <h3 class="modal__title">{{ title }}</h3>
      </div>
      <div class="modal__body">
        <div class="notice" :class="isL1 ? 'notice--warn' : 'notice--error'">
          {{ description }}
        </div>

        <div v-if="payload?.files?.length" class="conflict-files">
          <div v-for="f in payload.files" :key="f.path" class="conflict-file">{{ f.path }}</div>
        </div>

        <div class="opt-list">
          <template v-if="isL1">
            <div class="opt" @click="resolve('L1ExportPatch')">
              <div class="opt__radio" />
              <div>
                <div class="opt__t">导出 patch 文件</div>
                <div class="opt__d">将本地未推送的 commit 导出为 .patch 文件，供人工解决后重放</div>
              </div>
            </div>
            <div class="opt" @click="resolve('L1DiscardLocal')">
              <div class="opt__radio" />
              <div>
                <div class="opt__t">放弃本地未推送 commit</div>
                <div class="opt__d">丢弃本地未推送的变更，使用远程版本继续同步</div>
              </div>
            </div>
          </template>
          <template v-else>
            <div class="opt" @click="resolve('L2KeepLocal')">
              <div class="opt__radio" />
              <div>
                <div class="opt__t">保留本地版本</div>
                <div class="opt__d">用本地配置覆盖远程 _current/，本地变更优先</div>
              </div>
            </div>
            <div class="opt" @click="resolve('L2KeepRemote')">
              <div class="opt__radio" />
              <div>
                <div class="opt__t">保留远程版本</div>
                <div class="opt__d">用远程 _current/ 覆盖本地配置目录</div>
              </div>
            </div>
            <div class="opt opt--disabled" aria-disabled="true">
              <div class="opt__radio" />
              <div>
                <div class="opt__t">手动合并</div>
                <div class="opt__d">手动合并编辑器（开发中，暂不可用）</div>
              </div>
            </div>
          </template>
        </div>
      </div>
      <div class="modal__foot">
        <button class="btn btn--ghost" :disabled="resolving" @click="cancel">取消同步</button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.opt--disabled {
  opacity: 0.5;
  cursor: not-allowed;
  pointer-events: none;
}
.opt--disabled .opt__radio {
  background: var(--surface-2);
}
</style>
