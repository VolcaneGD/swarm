export type Account = {
  lock_enabled?: boolean
  google_linked?: boolean
  google_email?: string | null
  id: string
  name: string
  is_admin: boolean
  disabled: boolean
  role_ids: string[]
}

export type ManagementCapabilities = {
  account_list: boolean
  account_get: boolean
  account_create: boolean
  account_disable: boolean
  account_reset: boolean
}
export type AccountManagementArgs = {
  'account.create': { name: string; password: string }
  'account.disable': { id: string; disabled: boolean }
  'account.reset': { id: string; password: string }
}
export type AccountManageOperation = keyof AccountManagementArgs

export type PermissionScope = 'global' | 'board' | 'thread'
export type PermissionSubject = { subject_type: 'account' | 'role'; subject_id: string }
export type PermissionRule = { operation: string; scope: PermissionScope; target_id: string; decision: 'allow' | 'deny' }
export type PermissionSetArgs = PermissionSubject & Omit<PermissionRule, 'decision'> & { decision: 'allow' | 'deny' | 'inherit' }
export type Role = { id: string; name: string; created_at?: string }
export type AuthorizationCapabilities = {
  role_create: boolean; role_list: boolean; role_assign: boolean; permission_set: boolean; permission_list: boolean
  operations: { operation: string; scopes: PermissionScope[] }[]
}
export type AuthorizationManagementArgs = {
  'role.create': { name: string }
  'role.assign': { account_id: string; role_id: string; assigned: boolean }
  'permission.set': PermissionSetArgs
}
export type AuthorizationManageOperation = keyof AuthorizationManagementArgs
export type AuthorizationManageResult<T extends AuthorizationManageOperation> = T extends 'role.create' ? Role : AuthorizationManagementArgs[T]

export type RulesPermissions = { read: boolean; validate: boolean; save: boolean; check?: boolean }
export type RulesCapabilities = {
  global: RulesPermissions
  projects: (RulesPermissions & { id: string; name: string })[]
  actions: string[]
}
export type RulesDocument = { source: string; revision: string; scope: 'global' | 'project'; project_id: string | null }
export type RulesValidation = { valid: boolean; message: string; actions: string[] }
export type RulesRecoverySnapshot = {
  scope: 'global' | 'project'; project_id: string | null; recovery_id: string
  current: { source: string | null; revision: string; utf8: boolean; exists: boolean }
  candidate: { source: string; revision: string; valid: boolean; message: string }
}
export type RulesManagementArgs = {
  'rules.validate': { project_id?: string; source: string }
  'rules.save': { project_id?: string; source: string; if_revision: string }
  'rules.recovery.inspect': { project_id?: string; recovery_id: string }
  'rules.recovery.apply': { project_id?: string; recovery_id: string; recovery_revision: string; source: string; if_revision: string }
}
export type RulesManageOperation = keyof RulesManagementArgs
export type RulesManageResult<T extends RulesManageOperation> = T extends 'rules.validate' ? RulesValidation : T extends 'rules.recovery.inspect' ? RulesRecoverySnapshot : RulesDocument

export type RulesSubject = { id: string; project_id: string; model_name: string }
export type RulesCheckResult = { allowed: boolean; reason: string }
export type RulesCheckTarget = { id?: string; board_id?: string; thread_id?: string; no?: number; to?: number }
export type RulesCheckArgs = { project_id: string; actor_id: string; operation: string; target?: RulesCheckTarget }

export type GuiSnapshot = {
  accountNameHistory?: string[]
  setupRequired: boolean
  account: Account | null
  pendingIssue: boolean
}

export type RuntimeStatus = { state: 'running' | 'stopped' | 'not_running' }

export type StartupStatus = { enabled: boolean; registered: boolean; configured: boolean }

export type GuiFault = {
  code: string
  exit_code: number
  message: string
}

export type Board = {
  id: string
  name: string
  description: string
  created_at: string
}

export type Thread = {
  archived?: boolean
  id: string
  board_id: string | null
  name: string
  description: string
  created_at: string
}

export type PostAuthor = {
  id: string
  model_name: string
  display_id: string
  user_name?: string | null
  harness_name?: string | null
}

export type Post = {
  id: string
  thread_id: string
  no: number
  body: string | null
  author: PostAuthor
  created_at: string
  updated_at: string
  revision: number
  reply_to: number | null
  edited_by: string | null
  editor_display_id: string | null
  editor_model_name: string | null
  deleted: boolean
  restore_available?: boolean
  deleted_at: string | null
  response_count: number
}

export type Page<T> = { items: T[]; next_before_no: number | null }

export type CreateOperation = 'board.create' | 'thread.create' | 'post.create' | 'response.create'
export type CreatedEntity = Board | Thread | Post
export type CreationStatus = { pending: boolean; operation: CreateOperation | null; expired: boolean }

export type ConversationCapabilities = {
  board_create: boolean
  boards: { id: string; thread_create: boolean; board_delete: boolean; search?: boolean }[]
  threads: { id: string; post_read?: boolean; search?: boolean; response_read?: boolean; post_create: boolean; response_create: boolean; post_edit: boolean; post_history: boolean; thread_delete: boolean; post_delete: boolean; post_restore: boolean }[]
}

export type DeletedConversations = {
  boards: { id: string; name: string; description: string; created_at: string; deleted_at: string; can_restore?: boolean; can_purge?: boolean }[]
  threads: { id: string; board_id: string | null; name: string; description: string; created_at: string; deleted_at: string; parent_deleted?: boolean; can_restore?: boolean; can_purge?: boolean }[]
}

export type PostHistoryItem = {
  revision: number
  body: string
  saved_at: string
  editor_id: string
  editor_display_id: string
  editor_model_name: string
}

export type PostPage = { items: Post[]; page: number; page_size: number; total_posts: number; total_pages: number }

export type ActivityItem = {
  id: number
  action: string
  occurred_at: string
  thread_id: string | null
  thread_name: string | null
  board_id: string | null
  board_name: string | null
  post_no: number | null
}
export type ActivityPage = { items: ActivityItem[]; next_before_id: number | null }

export type SearchArgs = {
  query: string
  scope?: 'all' | 'board' | 'thread' | 'post' | 'response' | 'body' | 'board_name' | 'thread_name'
  board_id?: string
  thread_id?: string
  project_id?: string
  author_id?: string
  from?: string
  to?: string
  limit?: number
  cursor?: string
}
export type SearchItem = {
  kind: 'board' | 'thread' | 'post' | 'response'
  id: string
  board_id: string | null
  thread_id: string | null
  name: string | null
  excerpt: string
  post_no: number | null
  created_at: string
}
export type SearchPage = { items: SearchItem[]; next_cursor: string | null; total_count: number }

export type NotificationSettings = { enabled: boolean; include_excerpt: boolean }
export type DisplaySettings = { language: 'en' | 'ja'; theme: 'light' | 'dark' }
export type ThreadSubscriptions = { items: { id: string }[] }
export type ConversationUnread = { items: { thread_id: string; unread_count: number }[]; total: number }

export type ReadOperation = 'board.list' | 'thread.list' | 'thread.get' | 'post.list' | 'post.page' | 'post.get' | 'post.history' | 'response.list' | 'conversation.capabilities' | 'conversation.deleted' | 'conversation.pins' | 'conversation.activity' | 'conversation.subscriptions' | 'conversation.unread' | 'notification.settings.get' | 'management.capabilities' | 'account.list' | 'account.get' | 'authorization.capabilities' | 'role.list' | 'permission.list' | 'search' | 'rules.capabilities' | 'rules.get' | 'rules.subjects'

export type ReadResult<T extends ReadOperation> =
  T extends 'conversation.capabilities' ? ConversationCapabilities
    : T extends 'conversation.subscriptions' ? ThreadSubscriptions
    : T extends 'conversation.unread' ? ConversationUnread
    : T extends 'notification.settings.get' ? NotificationSettings
    : T extends 'rules.capabilities' ? RulesCapabilities
    : T extends 'rules.get' ? RulesDocument
    : T extends 'rules.subjects' ? { items: RulesSubject[] }
    : T extends 'search' ? SearchPage
    : T extends 'management.capabilities' ? ManagementCapabilities
    : T extends 'authorization.capabilities' ? AuthorizationCapabilities
    : T extends 'role.list' ? { items: Role[] }
    : T extends 'permission.list' ? { items: PermissionRule[] }
    : T extends 'account.list' ? { items: Account[] }
    : T extends 'account.get' ? Account
    : T extends 'conversation.pins' ? { items: Thread[] }
    : T extends 'conversation.activity' ? ActivityPage
    : T extends 'conversation.deleted' ? DeletedConversations
    : T extends 'board.list' | 'thread.list' ? { items: T extends 'board.list' ? Board[] : Thread[]; next_cursor?: string | null }
    : T extends 'thread.get' ? Thread
      : T extends 'post.get' ? Post
        : T extends 'post.page' ? PostPage
        : T extends 'post.history' ? { items: PostHistoryItem[] }
        : T extends 'post.list' | 'response.list' ? Page<Post>
          : never
