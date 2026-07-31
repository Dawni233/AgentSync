// 前后端共享类型定义
// 与 Rust 端 src-tauri/src/types.rs（Phase 2+ 落地）一一对应

/* ------------------------------------------------------------------ */
/* Agent                                                               */
/* ------------------------------------------------------------------ */

/** 用户新增 agent 时填写的表单数据 */
export interface AgentConfig {
  /** agent 标识符，如 "workbuddy"，作为仓库存放目录名 */
  id: string
  /** UI 显示名，如 "WorkBuddy" */
  displayName: string
  /** 本地配置目录绝对路径或 ~ 开头，如 "~/.workbuddy" */
  configDir: string
  /** 包含 glob 规则 */
  syncFiles: string[]
  /** 排除 glob 规则 */
  excludeFiles: string[]
  /** UI 标识色（可选），如 "#42b883" */
  accentColor?: string
}

/** 注册后含运行时状态的完整 agent 对象 */
export interface Agent extends AgentConfig {
  /** 当前激活的人格名，未激活则为 null */
  currentPersona: string | null
  /** 运行时同步状态 */
  syncStatus: SyncStatus
  /** 最近一次同步时间戳（ms），未同步为 null */
  lastSyncAt: number | null
  /** 跟踪文件数（syncFiles 匹配 - excludeFiles 匹配） */
  trackedFileCount: number
}

export type SyncStatus = 'idle' | 'syncing' | 'pending' | 'conflict' | 'error'
// idle=已同步，pending=本地有变更待同步，syncing=同步中，conflict/error=异常

/* ------------------------------------------------------------------ */
/* Settings                                                            */
/* ------------------------------------------------------------------ */

/** 应用全局设置，存 SQLite settings 表（单行） */
export interface Settings {
  /** 远程仓库 URL，如 https://gitee.com/user/workbuddy-sync.git */
  repoUrl: string
  platform: 'github' | 'gitee'
  /** PAT，明文存（MVP），未来迁移 keychain */
  patToken: string
  autoSyncEnabled: boolean
  /** 自动同步间隔（分钟），可选值 5/15/30/60 */
  autoSyncIntervalMin: number
  /** 开机自启动 */
  launchAtLogin: boolean
}

/* ------------------------------------------------------------------ */
/* SyncResult                                                          */
/* ------------------------------------------------------------------ */

/** 单次 sync_agent 的返回值；sync_all 返回 SyncResult[] */
export interface SyncResult {
  agentId: string
  status: 'success' | 'conflict' | 'error' | 'skipped'
  /** 从远程拉取并写回本地的文件（相对 agent 目录路径） */
  pulledFiles: string[]
  /** 本地变更推送到远程的文件 */
  pushedFiles: string[]
  /** 冲突文件（status='conflict' 时填充） */
  conflictFiles: string[]
  /** status='error' 时填充 */
  errorMessage?: string
  /** 同步耗时（ms） */
  durationMs: number
}

/* ------------------------------------------------------------------ */
/* Persona                                                             */
/* ------------------------------------------------------------------ */

/** 人格（角色）元数据，从 agent 文件夹扫描得到 */
export interface Persona {
  agentId: string
  /** 文件夹名，如 "work-mode" */
  name: string
  /** 从 manifest 或文件夹名生成 */
  displayName: string
  /** 包含的文件列表（相对 agent 目录） */
  files: string[]
  /** 总字节数 */
  sizeBytes: number
  /** 是否当前激活 */
  isCurrent: boolean
  /** 导入来源标记，导入时填充 */
  importedAt?: number
}

/** 人格文件预览内容（read_persona_file 返回值） */
export interface PersonaFileContent {
  /** 人格快照中的文件内容；不存在/编码异常/二进制时为 null */
  personaContent: string | null
  /** 本地配置目录对应文件内容；不存在/编码异常/二进制时为 null */
  localContent: string | null
  /** 是否二进制文件（含 0x00 字节） */
  isBinary: boolean
}

/* ------------------------------------------------------------------ */
/* 事件 payload                                                        */
/* ------------------------------------------------------------------ */

export interface SyncStartedPayload {
  agentId: string
}

export interface SyncProgressPayload {
  agentId: string
  step: number
  stepName: string
}

export interface SyncCompletedPayload {
  result: SyncResult
}

export interface SyncErrorPayload {
  agentId: string
  errorMessage: string
}

export interface ConflictDetectedPayload {
  agentId: string
  conflictType: 'L1' | 'L2'
  files: Array<{ path: string; localMtime: number; remoteMtime: number }>
}
