import { reactive } from 'vue'

export interface ToastItem {
  id: number
  msg: string
}

// 模块级单例：任意组件 import showToast 即可触发，App.vue 通过 useToasts 渲染
const items = reactive<ToastItem[]>([])
let seq = 0

export function showToast(msg: string, duration = 2400): void {
  const id = ++seq
  items.push({ id, msg })
  window.setTimeout(() => {
    const i = items.findIndex((t) => t.id === id)
    if (i !== -1) items.splice(i, 1)
  }, duration)
}

export function useToasts() {
  return items
}
