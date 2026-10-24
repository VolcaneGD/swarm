import { useSyncExternalStore } from 'react'

export type OperationStatus = { id: number; operation: string; state: 'pending' | 'completed' | 'failed' }
let sequence = 0
let epoch = 0
let entries: OperationStatus[] = []
const listeners = new Set<() => void>()
const publish = () => { for (const listener of listeners) listener() }
const subscribe = (listener: () => void) => { listeners.add(listener); return () => { listeners.delete(listener) } }
export const getOperationStatuses = () => entries
export function clearOperationStatuses() { epoch++; entries = []; publish() }
export function useOperationStatuses() { return useSyncExternalStore(subscribe, getOperationStatuses) }

// Only operation names and outcomes are retained in memory, never arguments,
// returned data, bodies, paths, account identities, or error details.
export async function trackOperation<T>(operation: string, run: () => Promise<T>): Promise<T> {
  const id = ++sequence
  const startedEpoch = epoch
  entries = [...entries, { id, operation, state: 'pending' }]
  publish()
  const finish = (state: 'completed' | 'failed') => {
    if (startedEpoch !== epoch) return
    entries = entries.map((entry) => entry.id === id ? { ...entry, state } : entry)
    const finished = entries.filter((entry) => entry.state !== 'pending' && (entry.id === id || entry.operation !== operation || entry.state !== state)).slice(-100)
    entries = [...finished, ...entries.filter((entry) => entry.state === 'pending')]
    publish()
  }
  try { const result = await run(); finish('completed'); return result }
  catch (error) { finish('failed'); throw error }
}

const labels: Record<string, [string, string]> = {
  'board.list': ['Boardsを読み込み', 'Loading Boards'], 'thread.list': ['Threadsを読み込み', 'Loading Threads'],
  'thread.get': ['Threadを読み込み', 'Loading Thread'], 'post.page': ['Postsを読み込み', 'Loading Posts'],
  'post.list': ['Postsを読み込み', 'Loading Posts'], 'post.get': ['Postを読み込み', 'Loading Post'],
  'response.list': ['Responsesを読み込み', 'Loading Responses'], 'post.history': ['編集履歴を読み込み', 'Loading edit history'],
  'board.create': ['Boardを作成', 'Creating Board'], 'thread.create': ['Threadを作成', 'Creating Thread'],
  'post.create': ['Postを投稿', 'Posting Post'], 'response.create': ['Responseを投稿', 'Posting Response'],
  'gui_edit': ['Postを編集', 'Editing Post'], 'search': ['検索', 'Searching'],
  'conversation.capabilities': ['会話の権限を確認', 'Checking conversation permissions'],
  'conversation.pins': ['ピン留めを読み込み', 'Loading pinned Threads'], 'conversation.activity': ['更新履歴を読み込み', 'Loading activity'],
  'conversation.deleted': ['削除済み項目を読み込み', 'Loading deleted items'],
  'conversation.subscriptions': ['購読状態を読み込み', 'Loading subscriptions'], 'conversation.unread': ['未読状態を読み込み', 'Loading unread status'],
  'thread.pin': ['ピン留めを保存', 'Saving pin'], 'thread.subscribe': ['購読状態を保存', 'Saving subscription'], 'post.read.mark': ['既読状態を保存', 'Saving read status'],
  'ui.preferences.get': ['画面設定を読み込み', 'Loading view preferences'], 'ui.preferences.set': ['画面設定を保存', 'Saving view preferences'],
  'gui_snapshot': ['ワークスペースへ接続', 'Connecting workspace'], 'gui_runtime_status': ['サービス状態を確認', 'Checking service status'],
  'gui_runtime_start': ['サービスを準備', 'Preparing service'], 'gui_runtime_stop': ['サービスを停止', 'Stopping service'],
  'gui_creation_status': ['作成状態を確認', 'Checking creation status'], 'gui_retry_create': ['作成を再試行', 'Retrying creation'],
  'gui_ack_create': ['作成結果を確認', 'Acknowledging creation'], 'gui_discard_create': ['作成要求を取り消し', 'Discarding creation'],
  'gui_open_link': ['リンクを開く処理を実行', 'Opening link'],
  'gui_notification_activation': ['通知の起動要求を確認', 'Checking notification activation'], 'gui_notification_ack': ['通知を確認', 'Acknowledging notification'],
  'gui_display_defaults': ['表示設定を読み込み', 'Loading display defaults'], 'display.settings.get': ['表示設定を読み込み', 'Loading display settings'], 'display.settings.set': ['表示設定を保存', 'Saving display settings'],
  'notification.settings.get': ['通知設定を読み込み', 'Loading notification settings'], 'notification.settings.set': ['通知設定を保存', 'Saving notification settings'],
  'management.capabilities': ['管理権限を確認', 'Checking management permissions'], 'authorization.capabilities': ['権限管理の可否を確認', 'Checking authorization capabilities'],
  'account.list': ['Accountsを読み込み', 'Loading Accounts'], 'account.get': ['Accountを読み込み', 'Loading Account'], 'role.list': ['Rolesを読み込み', 'Loading Roles'], 'permission.list': ['権限を読み込み', 'Loading permissions'],
  'role.create': ['Roleを作成', 'Creating Role'], 'role.assign': ['Roleを割り当て', 'Assigning Role'], 'permission.set': ['権限を保存', 'Saving permissions'],
  'permission.batch.save': ['権限の変更を保存', 'Saving permission changes'],
  'rules.get': ['RULESを読み込み', 'Loading RULES'], 'rules.save': ['RULESを保存', 'Saving RULES'], 'rules.validate': ['RULESを検証', 'Validating RULES'], 'rules.subjects': ['RULESの対象を読み込み', 'Loading RULES subjects'], 'rules.capabilities': ['RULESの権限を確認', 'Checking RULES permissions'], 'gui_rules_check': ['RULESの判定を確認', 'Checking RULES decision'],
  'project.list': ['Projectsを読み込み', 'Loading Projects'], 'project.register': ['Projectを登録', 'Registering Project'],
  'retention.settings.get': ['保持設定を読み込み', 'Loading retention settings'], 'retention.settings.set': ['保持設定を保存', 'Saving retention settings'],
  'gui_startup_status': ['自動起動設定を確認', 'Checking startup settings'], 'gui_startup_set': ['自動起動設定を保存', 'Saving startup settings'],
}
export function operationStatusLabel(entry: OperationStatus, language: 'ja' | 'en') {
  const label = labels[entry.operation]
  if (language === 'ja') return label ? `${label[0]}${entry.state === 'pending' ? '中' : entry.state === 'completed' ? '完了' : '失敗'}` : `${entry.operation}：${entry.state === 'pending' ? '処理中' : entry.state === 'completed' ? '完了' : '失敗'}`
  return `${label?.[1] || entry.operation} · ${entry.state === 'pending' ? 'In progress' : entry.state === 'completed' ? 'Completed' : 'Failed'}`
}
