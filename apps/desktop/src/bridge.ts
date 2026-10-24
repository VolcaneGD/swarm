import { invoke as nativeInvoke } from '@tauri-apps/api/core'
import { trackOperation } from './operationStatus'

function invoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  const operation = typeof args?.operation === 'string' ? `${command === 'gui_backup_control' ? 'backup.' : ''}${args.operation}` : command
  return trackOperation(operation, () => nativeInvoke<T>(command, args))
}
import type { CreatedEntity, CreateOperation, CreationStatus, GuiSnapshot, Post, ReadOperation, ReadResult, RuntimeStatus, StartupStatus } from './types'
import type { Account, AccountManagementArgs, AccountManageOperation } from './types'
import type { AuthorizationManageOperation, AuthorizationManagementArgs, AuthorizationManageResult } from './types'
import type { RulesManageOperation, RulesManagementArgs, RulesManageResult, RulesCheckArgs, RulesCheckResult } from './types'
import type { NotificationSettings, Thread } from './types'

export type AuthMode = 'setup' | 'login'

export type UiPreferenceScope = 'layout' | 'conversation_view' | 'thread_scroll'
export function guiPreferenceGet(accountId: string, args: { scope: UiPreferenceScope; key?: string }): Promise<{ value: unknown | null; migration_allowed: boolean }> {
  return invoke('gui_ui_preferences', { expectedAccountId: accountId, operation: 'ui.preferences.get', args })
}
export function guiPreferenceSet(accountId: string, args: { scope: UiPreferenceScope; key?: string; value: unknown }): Promise<{ saved: true }> {
  return invoke('gui_ui_preferences', { expectedAccountId: accountId, operation: 'ui.preferences.set', args })
}

export type RetentionSettings = { history_days: number | null; deleted_days: number | null; deleted_board_days?: number | null; deleted_thread_days?: number | null; archived_thread_days?: number | null }
export function guiRetentionSettings(accountId: string): Promise<RetentionSettings> {
  return invoke('gui_retention_control', { expectedAccountId: accountId, operation: 'retention.settings.get', args: {} })
}
export function guiSetRetentionSettings(accountId: string, settings: RetentionSettings): Promise<RetentionSettings> {
  return invoke('gui_retention_control', { expectedAccountId: accountId, operation: 'retention.settings.set', args: settings })
}

export type Project = { id: string; name: string; canonical_root: string; paths: string[]; created_at: string }
export function guiProjectList(accountId: string): Promise<{ items: Project[] }> {
  return invoke('gui_project_control', { expectedAccountId: accountId, operation: 'project.list', args: {} })
}
export function guiProjectRegister(accountId: string, name: string, root: string): Promise<Project> {
  return invoke('gui_project_control', { expectedAccountId: accountId, operation: 'project.register', args: { name, root } })
}

export function guiGoogleLink(accountId: string, currentPassword: string): Promise<GuiSnapshot> {
  return invoke('gui_google_link', { expectedAccountId: accountId, currentPassword })
}
export function guiGoogleUnlink(accountId: string, currentPassword: string): Promise<GuiSnapshot> {
  return invoke('gui_google_unlink', { expectedAccountId: accountId, currentPassword })
}
export function guiGoogleRecover(name: string, password: string): Promise<GuiSnapshot> {
  return invoke('gui_google_recover', { name, password })
}

export type BackupSettings = { enabled: boolean; interval_seconds: number; retention: number; destination: string | null }
export type RestoreTarget = { project_id: string | null; target_path: string; revision: string }
export type RestorePreview = {
  input_sha256: string; targets: RestoreTarget[]
  manifest: { created_at: string; schema_version: number; database_bytes: number; rules: { project_id: string | null; source_path: string; content: string | null; revision: string }[] }
}
export type RestoreStatus = {
  state: 'selected' | 'preparing' | 'prepared' | 'unavailable' | 'applied' | 'rolled_back'; path: string; preview: RestorePreview | null; error_code: string | null; can_retry?: boolean
  preparation: { intent: string; state: 'safety_pending' | 'prepared' | 'applied' | 'rolled_back'; safety_directory: string; safety_key: string; safety_sha256?: string | null } | null
}
export type RestoreOutcome = 'applied' | 'rolled_back' | 'uncertain'
export function guiApplyRestore(accountId: string): Promise<{ state: 'applied' | 'rolled_back'; reauthentication_required: true }> { return invoke('gui_restore_apply', { expectedAccountId: accountId, confirmed: true }) }
export function guiRestoreStatus(accountId: string): Promise<RestoreStatus | null> { return invoke('gui_restore_status', { expectedAccountId: accountId }) }
export function guiPickRestoreBackup(accountId: string): Promise<RestoreStatus | null> { return invoke('gui_restore_pick', { expectedAccountId: accountId }) }
export function guiPrepareRestore(accountId: string): Promise<RestoreStatus> { return invoke('gui_restore_prepare', { expectedAccountId: accountId, confirmed: true }) }
export function guiClearRestore(accountId: string): Promise<null> { return invoke('gui_restore_clear', { expectedAccountId: accountId }) }
export type BackupGeneration = { key: string; destination: string }
export type ManualBackupResult = { intent: string; state: 'not_started' | 'pending' | 'completed' | 'unavailable'; key?: string; destination?: string }
export type BackupStatus = {
  manual?: ManualBackupResult | null
  settings: BackupSettings
  default_destination: string
  journal_error: string | null
  pending_stage: 'publication_pending' | 'receipt_pending' | 'unavailable' | null
  schedule: { version: number; last_success_at: number | null; pending: { generation: BackupGeneration; due_at: number } | null; generations: BackupGeneration[]; prune: BackupGeneration[] } | null
}
export function guiBackupStatus(accountId: string): Promise<BackupStatus> {
  return invoke('gui_backup_control', { expectedAccountId: accountId, operation: 'status', args: {} })
}
export type BackupArchive = { directory: string; key: string; bytes: number; deletable: boolean }
export type StorageInventory = { archives: BackupArchive[]; recovery_bytes: number }
export function guiStorageInventory(accountId: string): Promise<StorageInventory> {
  return invoke('gui_backup_control', { expectedAccountId: accountId, operation: 'archives.list', args: {} })
}
export function guiDeleteBackupArchive(accountId: string, archive: BackupArchive): Promise<{ deleted: boolean }> {
  return invoke('gui_backup_control', { expectedAccountId: accountId, operation: 'archives.delete', args: { directory: archive.directory, key: archive.key, confirmed: true } })
}
export function guiClearRestoreArtifacts(accountId: string): Promise<{ deleted: boolean }> {
  return invoke('gui_backup_control', { expectedAccountId: accountId, operation: 'restore.cleanup', args: { confirmed: true } })
}
export function guiSetBackupSettings(accountId: string, settings: BackupSettings): Promise<BackupSettings> {
  return invoke('gui_backup_control', { expectedAccountId: accountId, operation: 'settings.set', args: settings })
}
export function guiCreateManualBackup(accountId: string): Promise<ManualBackupResult> {
  return invoke('gui_backup_control', { expectedAccountId: accountId, operation: 'manual.create', args: {} })
}
export function guiAckManualBackup(accountId: string, intent: string): Promise<{ acknowledged: boolean }> {
  return invoke('gui_backup_control', { expectedAccountId: accountId, operation: 'manual.ack', args: { intent } })
}

export type NotificationActivation =
  | { state: 'none' }
  | { state: 'login_required' | 'account_required' | 'unavailable'; event_id: string }
  | { state: 'ready'; event_id: string; account_id: string; thread: Thread }

export function guiNotificationActivation(): Promise<NotificationActivation> {
  return invoke('gui_notification_activation')
}

export function guiNotificationAck(eventId: string): Promise<boolean> {
  return invoke('gui_notification_ack', { eventId })
}

export function getSnapshot(): Promise<GuiSnapshot> {
  return invoke<GuiSnapshot>('gui_snapshot')
}

export function guiRuntimeStatus(): Promise<RuntimeStatus> {
  return invoke<RuntimeStatus>('gui_runtime_status')
}

export function guiRuntimeStart(): Promise<RuntimeStatus> {
  return invoke<RuntimeStatus>('gui_runtime_start')
}

export function guiRuntimeStop(): Promise<RuntimeStatus> {
  return invoke<RuntimeStatus>('gui_runtime_stop')
}

export function guiStartupStatus(): Promise<StartupStatus> {
  return invoke<StartupStatus>('gui_startup_status')
}

export function guiStartupSet(enabled: boolean): Promise<StartupStatus> {
  return invoke<StartupStatus>('gui_startup_set', { enabled })
}

export function authenticate(mode: AuthMode, name: string, password: string): Promise<GuiSnapshot> {
  return invoke<GuiSnapshot>('gui_authenticate', { mode, name, password })
}

export function retryPending(): Promise<GuiSnapshot> {
  return invoke<GuiSnapshot>('gui_retry_pending')
}

export function guiSetPassword(accountId: string, currentPassword: string, password: string): Promise<GuiSnapshot> {
  return invoke('gui_set_password', { expectedAccountId: accountId, currentPassword, password })
}

export function recoverAccount(name: string, recoveryCode: string, password: string): Promise<GuiSnapshot> {
  return invoke<GuiSnapshot>('gui_recover_account', { name, recoveryCode, password })
}

export function cancelPending(): Promise<GuiSnapshot> {
  return invoke<GuiSnapshot>('gui_cancel_pending')
}

export function revealRecoveryCode(): Promise<{ recovery_code: string }> {
  return invoke('gui_reveal_recovery_code')
}

export function guiAuthorizationManage<T extends AuthorizationManageOperation>(operation: T, args: AuthorizationManagementArgs[T]): Promise<AuthorizationManageResult<T>> {
  return invoke('gui_authorization_manage', { operation, args })
}

export function guiRulesManage<T extends RulesManageOperation>(operation: T, args: RulesManagementArgs[T]): Promise<RulesManageResult<T>> {
  return invoke('gui_rules_manage', { operation, args })
}

export function guiRulesCheck(args: RulesCheckArgs, context: Record<string, unknown>): Promise<RulesCheckResult> {
  return invoke('gui_rules_check', { args, context })
}

export function logout(): Promise<GuiSnapshot> {
  return invoke<GuiSnapshot>('gui_logout')
}

export function guiOpenLink(url: string): Promise<void> {
  return invoke<void>('gui_open_link', { url })
}

export function guiRead<T extends ReadOperation>(operation: T, args: Record<string, unknown> = {}): Promise<ReadResult<T>> {
  return invoke<ReadResult<T>>('gui_read', { operation, args })
}

export function guiCreate(operation: CreateOperation, args: Record<string, unknown>): Promise<CreatedEntity> {
  return invoke<CreatedEntity>('gui_create', { operation, args })
}

export function guiEdit(args: { thread_id: string; no: number; body: string; if_revision: number }): Promise<Post> {
  return invoke<Post>('gui_edit', { args })
}

export type ManageOperation = 'board.delete' | 'board.restore' | 'thread.delete' | 'thread.restore' | 'post.delete' | 'post.restore' | 'board.purge' | 'thread.purge'
export type ConversationPurgeResult = { kind: 'board' | 'thread'; id: string; removed_threads: number; removed_posts: number; removed_revisions: number; storage_reclaimed?: boolean }
export type ManageResult = Post | { id: string; deleted: boolean } | ConversationPurgeResult

export function guiManage(operation: ManageOperation, args: { id: string; confirmed?: true } | { thread_id: string; no: number }): Promise<ManageResult> {
  return invoke<ManageResult>('gui_manage', { operation, args })
}

export type ConversationBatchArgs = { action: 'restore' | 'purge'; items: { kind: 'board' | 'thread'; id: string }[]; confirmed: true }
export function guiManageConversationBatch(args: ConversationBatchArgs): Promise<{ action: 'restore' | 'purge'; items: { kind: 'board' | 'thread'; id: string }[] }> {
  return invoke('gui_manage', { operation: 'conversation.batch', args })
}

export function guiPinThread(id: string, pinned: boolean): Promise<{ id: string; pinned: boolean }> {
  return invoke('gui_manage', { operation: 'thread.pin', args: { id, pinned } })
}

export function guiSubscribeThread(accountId: string, id: string, subscribed: boolean): Promise<{ id: string; subscribed: boolean }> {
  return invoke('gui_notification_manage', { expectedAccountId: accountId, operation: 'thread.subscribe', args: { id, subscribed } })
}

// Call only for stable IDs captured from posts actually displayed by the GUI.
export function guiMarkDisplayedPostsRead(accountId: string, threadId: string, postIds: string[]): Promise<{ marked: number }> {
  return invoke('gui_notification_manage', { expectedAccountId: accountId, operation: 'post.read.mark', args: { thread_id: threadId, post_ids: postIds } })
}

export function guiSetNotificationSettings(accountId: string, settings: NotificationSettings): Promise<NotificationSettings> {
  return invoke('gui_notification_manage', { expectedAccountId: accountId, operation: 'notification.settings.set', args: settings })
}

export function guiDisplayDefaults(): Promise<import('./types').DisplaySettings> { return invoke('gui_display_defaults') }
export function guiDisplaySettings(): Promise<{ settings: import('./types').DisplaySettings | null }> { return invoke('gui_read', { operation: 'display.settings.get', args: {} }) }
export function guiSetDisplaySettings(accountId: string, settings: import('./types').DisplaySettings): Promise<import('./types').DisplaySettings> {
  return invoke('gui_notification_manage', { expectedAccountId: accountId, operation: 'display.settings.set', args: settings })
}

export function guiAccountManage<T extends AccountManageOperation>(operation: T, args: AccountManagementArgs[T]): Promise<Account> {
  return invoke<Account>('gui_account_manage', { operation, args })
}

export function guiCreationStatus(): Promise<CreationStatus> {
  return invoke<CreationStatus>('gui_creation_status')
}

export function guiRetryCreate(): Promise<CreatedEntity> {
  return invoke<CreatedEntity>('gui_retry_create')
}

export function guiAckCreate(): Promise<CreationStatus> {
  return invoke<CreationStatus>('gui_ack_create')
}

export function guiDiscardCreate(): Promise<CreationStatus> {
  return invoke<CreationStatus>('gui_discard_create')
}
