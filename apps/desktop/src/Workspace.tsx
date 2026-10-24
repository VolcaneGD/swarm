import { useCallback, useEffect, useId, useLayoutEffect, useMemo, useRef, useState, type CSSProperties, type FormEvent, type KeyboardEvent as ReactKeyboardEvent, type PointerEvent as ReactPointerEvent } from 'react'
import { guiAckCreate, guiCreate, guiCreationStatus, guiDiscardCreate, guiEdit, guiManage, guiManageConversationBatch, guiNotificationAck, guiNotificationActivation, guiOpenLink, guiPinThread, guiRead, guiRetryCreate, type ManageOperation } from './bridge'
import { COMPOSER_HEIGHT_MIN, DEFAULT_LAYOUT_PREFERENCES, layoutPreferencesKey, normalizeLayoutPreferences, parseLayoutPreferences } from './layoutPreferences'
import { DEFAULT_THREAD_SCROLL_PREFERENCE, normalizeThreadScrollPreference, parseThreadScrollPreference, threadScrollPreferenceKey, type ThreadScrollPreference } from './threadScrollPreferences'
import { pageNumberItems } from './pagination'
import { readAllPages } from './readAllPages'
import { BOARD_NAME_LIMIT, THREAD_NAME_LIMIT, BODY_LIMIT, characterCount, characterCounter } from './textLimits'
import { buildConversationTree, getConversationAncestors, visibleConversationRows } from './conversationTree'
import { conversationViewPreferenceKey, normalizeConversationView, parseConversationViewPreference, type ConversationView } from './conversationViewPreferences'
import { isDesktopPreferenceHost, loadUiPreference, saveUiPreference, type UiPreferenceScope } from './uiPreferenceStorage'
import { registerPreferenceFlush, registerUpdateBlocker } from './nativeClosePreferences'
import SearchDialog from './SearchDialog'
import MarkdownBody from './MarkdownBody'
import { localizedDate } from './localizedDate'
import UpdateNotice from './UpdateNotice'
import UpdateReleaseNotes from './UpdateReleaseNotes'
import type { AvailableUpdate, UpdateNotes } from './updates'
import useConversationNotifications from './useConversationNotifications'
import usePrivateViewGuard from './usePrivateViewGuard'
import useSettingsCapabilityGuard from './useSettingsCapabilityGuard'
import useDisplayedPostRead from './useDisplayedPostRead'
import UnreadBadge from './UnreadBadge'
import { useOperationStatuses, operationStatusLabel } from './operationStatus'
import { normalizeSafeExternalUrl } from './safe-links'
import { notifyRefreshFeedback } from './refreshFeedback'
import type { ActivityItem, Board, ConversationCapabilities, CreatedEntity, CreationStatus, CreateOperation, DeletedConversations, Post, PostHistoryItem, SearchItem, Thread } from './types'

type Language = 'en' | 'ja'
const refreshCopy = {
  ja: { current: '現スレッドを更新', all: '全スレッドを更新', pinned: 'ピン留めを更新', noCurrent: 'スレッドを開いてください', noThreads: 'スレッドがありません', noPins: 'ピン留めされたスレッドがありません' },
  en: { current: 'Refresh Current Thread', all: 'Refresh All Threads', pinned: 'Refresh Pinned Threads', noCurrent: 'Open a Thread', noThreads: 'No Threads are available', noPins: 'No pinned Threads are available' },
}
type PageState = 'loading' | 'ready' | 'failed'
type CapabilityState = 'loading' | 'ready' | 'failed'
type DraftReceipt = { operation: CreateOperation; draftKey: string | null; body: string | null; threadId: string | null }
type PostEditState = { threadId: string; no: number; baseRevision: number; draft: string; latest: Post | null; saving: boolean; reloading: boolean; conflict: boolean; reviewedLatest: boolean; error: string }
type PostHistoryState = { threadId: string; no: number; state: 'loading' | 'ready' | 'failed'; current: Post | null; items: PostHistoryItem[] }
type ScrollCommit = { threadId: string; generation: number; pageGeneration: number; preference: ThreadScrollPreference; mode: 'restore'; failed?: boolean; preservePreference?: boolean }
type PreferenceError = 'read' | 'write' | null
type PreferenceErrorSetter = (error: PreferenceError) => void
type ThreadPageState = { page: number; pageSize: 50 | 100 | 500 | 1000; totalPosts: number; totalPages: number }
type ThreadPageSize = ThreadPageState['pageSize']
type WorkspaceTab = 'boards' | 'pinned' | 'archive' | 'activity'
type ManagementTarget =
  | { kind: 'board'; id: string; name: string; action: 'delete' | 'restore' | 'purge' }
  | { kind: 'thread'; id: string; name: string; action: 'delete' | 'restore' | 'purge'; boardName: string | null }
  | { kind: 'post'; id: string; threadId: string; threadName: string; no: number; action: 'delete' | 'restore' }
type DeletedItemRef = { kind: 'board' | 'thread'; id: string }
type DeletedBulkAction = 'restore' | 'purge'
const deletedItemKey = (item: DeletedItemRef) => `${item.kind}:${item.id}`

const emptyCreationStatus: CreationStatus = { pending: false, operation: null, expired: false }

function isAccessFault(error: unknown): boolean {
  if (typeof error !== 'object' || error === null || !('exit_code' in error)) return false
  const exitCode = (error as { exit_code?: unknown }).exit_code
  return typeof exitCode === 'number' && [3, 4, 5].includes(exitCode)
}

function faultExitCode(error: unknown): number | null {
  if (typeof error !== 'object' || error === null || !('exit_code' in error)) return null
  const value = (error as { exit_code?: unknown }).exit_code
  return typeof value === 'number' ? value : null
}

function isThreadFullFault(error: unknown): boolean {
  return typeof error === 'object' && error !== null && 'code' in error && (error as { code?: unknown }).code === 'THREAD_FULL'
}

const words = {
  en: {
    boards: 'Boards', accessible: 'Accessible Threads', refresh: 'Refresh', loading: 'Loading workspace…', resizeBoards: 'Resize Boards pane', resizeComposer: 'Resize composer pane', quickStartTitle: 'Welcome to Swarm', quickStartIntro: 'Start a conversation in a few steps.', quickStartStepOne: 'Choose a Board from the Tree View.', quickStartStepTwo: 'Select a Thread to read the conversation.', quickStartStepThree: 'Create a Board or Thread from the toolbar when you have permission.', closeThread: 'Close Thread', pageTop: 'Page top', pageBottom: 'Page bottom', restoringPosition: 'Restoring Thread position…', restorePositionFailed: 'Could not restore the saved Thread position. Showing the saved page.', scrollStorageReadFailed: 'Thread position preferences could not be loaded.', scrollStorageWriteFailed: 'Thread position preferences could not be saved. The current position remains available in this session.', jumpFailed: 'Could not restore the saved Thread position. Showing the saved page.', layoutLoadFailed: 'Layout preferences could not be loaded. Default layout sizes are being used.', layoutSaveFailed: 'Layout preferences could not be saved. Current sizes remain available in this session.',
    unavailable: 'Could not load the workspace. Try refreshing.', noBoards: 'No Boards are available.',
    checkingPermissions: 'Checking permissions…', permissionFailed: 'Could not check permissions. Creation is read-only until you refresh.', noCreatePermission: 'You can read this Thread, but Post and Response creation are not permitted.', noBoardThreadPermission: 'Board and Thread creation are not permitted.',
    noThreads: 'No Threads in this Board.', emptyThreads: 'No accessible Threads are available.',
    select: 'Choose a Thread to read the conversation.', threadLoading: 'Loading Thread…',
    threadFailed: 'Could not load this Thread. Select it again or refresh.', noPosts: 'No Posts yet.',
    older: 'Load older Posts', loadingOlder: 'Loading older Posts…', responses: 'direct responses',
    noResponses: 'No direct responses.', responseFailed: 'Could not load direct responses.',
    replyTo: 'Reply to', deleted: 'Deleted Post', edited: 'Edited', unknown: 'Unknown', olderFailed: 'Could not load older Posts. Try again.',
    edit: 'Edit', history: 'History', editPost: 'Edit Post', cancel: 'Cancel', saveEdit: 'Save changes', savingEdit: 'Saving…',
    editBody: 'Post body', editConflict: 'This Post changed after you opened it. Reload the latest version and review it before saving again. Your draft is still here.',
    editCommunication: 'The edit request could not be confirmed. Your draft is still here. Check the latest version before sending again.', editFailed: 'Could not save this Post. Your draft is still here.',
    revisionInvalid: 'Could not confirm a valid current revision. Reload the workspace before editing this Post.',
    reloadLatest: 'Reload latest version', reloadingLatest: 'Reloading…', latestVersion: 'Latest saved version', draft: 'Your draft',
    editSaved: 'Changes saved.', historyTitle: 'Post history', closeHistory: 'Close history', currentVersion: 'Current version',
    beforeRevision: (revision: number) => `Before revision ${revision}`, updatedBy: 'Updated by', savedAt: 'Saved',
    historyEmpty: 'There are no earlier versions of this Post.', historyFailed: 'Could not load this Post’s history.', historyLoading: 'Loading history…',
    delete: 'Delete', restore: 'Restore', deletedItems: 'Deleted items', deletedItemsTitle: 'Deleted Boards and Threads', closeDeletedItems: 'Close deleted items', deletedItemsLoading: 'Loading deleted items…', deletedItemsFailed: 'Could not load deleted items.', noDeletedItems: 'There are no restorable Boards or Threads.', deletedBoards: 'Boards', deletedThreads: 'Threads', deletedAt: 'Deleted',
    permanentlyDelete: 'Permanently delete', selectDeleted: 'Select', finishSelecting: 'Done', selectAllDeleted: 'Select all', clearDeletedSelection: 'Clear selection', selectedDeletedCount: (count: number) => `${count} selected`, bulkRestore: 'Restore selected', bulkPurge: 'Permanently delete selected', confirmBulkRestoreTitle: 'Confirm restoration', confirmPurgeTitle: 'Confirm permanent deletion', bulkRestoreImpact: 'Only the listed items will be restored. A selected Board and its selected Threads are restored together. Other deleted Threads remain deleted.', purgeBoardImpact: 'This Board and all of its descendant Threads, Posts, revisions, and conversation state will be permanently removed. This cannot be undone.', purgeThreadImpact: 'This Thread, its Posts, revisions, and conversation state will be permanently removed. This cannot be undone.', bulkPurgeImpact: 'Only the listed targets will be purged. A listed Board also removes its descendant Threads and Posts.', parentMustBeSelected: 'Select the deleted parent Board with this Thread to restore both together.', bulkOperationFailed: 'The selected items could not be changed. The list was refreshed; review the selection and current items before retrying.', staleSelection: 'Some selected items are no longer in the refreshed list. Clear the selection and review the current items.',
    confirmDeleteTitle: 'Confirm deletion', confirmRestoreTitle: 'Confirm restoration', cancelManagement: 'Cancel', confirmManagement: 'Confirm', boardDeleteImpact: 'This Board and its Threads and Posts will be hidden until the Board is restored. Independently deleted Threads and Posts stay deleted.', threadDeleteImpact: 'This Thread and its Posts will be hidden until the Thread is restored. Independently deleted Posts stay deleted.', postDeleteImpact: 'This Post body will be hidden. Its number and deleted marker remain.', restoreImpact: 'Only this item will be restored. Items previously deleted on their own stay deleted.',
    managementFailed: 'The result could not be confirmed. The workspace was refreshed; review its current state before another action.', managementRefreshing: 'Refresh the workspace before another management action.',
    createBoard: 'Create Board', createThread: 'Create Thread', newBoard: 'New Board', newThread: 'New Thread', closeCreation: 'Close', settings: 'Settings', search: 'Search', searchNavigationFailed: 'This search result is no longer available or cannot be opened. Refresh the workspace and try again.',
    opBoard: 'Board creation', opThread: 'Thread creation', opPost: 'Post creation', opResponse: 'Response creation',
    boardName: 'Board name', threadName: 'Thread name', description: 'Description', parentBoard: 'Board', chooseBoard: 'Choose a Board',
    create: 'Create', postPlaceholder: 'Write a Post…', responsePlaceholder: 'Write a Response…', sendPost: 'Post', sendResponse: 'Reply',
    cancelReply: 'Cancel reply', bodyRequired: 'Enter a message before posting.', bodyTooLarge: 'The message exceeds the 1 MiB UTF-8 limit.',
    nameRequired: 'Enter a name.', creationFailed: 'Could not create this item. Your Post draft is still here.',
    ackPending: 'The item was created. Confirmation is still pending; retry to finish safely. Your draft remains available.',
    replyUnavailable: 'This Post is no longer available for replies.',
    creationPending: 'A creation request is waiting for a result. Retrying may complete an item that already exists.',
    discardCaution: 'It may already have completed; discard removes only the retry record.', creationExpired: 'This request is older than 7 days and cannot be resent automatically. Discard the retry record to continue.',
    retryCreate: 'Retry saved creation', discardCreate: 'Discard retry record', checkCreation: 'Check creation status', creationUnavailable: 'Creation status is unavailable. Recheck before creating or logging out.',
    created: 'Created successfully.', byteCount: (count: number) => `${count.toLocaleString()} / 1,048,576 bytes`,
    nameTooLong: (limit: number) => `Use ${limit} characters or fewer for the name.`, bodyTooLong: 'Use 100,000 characters or fewer for the message.',
    externalLinkTitle: 'Open external link?', externalLinkExplanation: 'This link will open outside Swarm.', externalLinkOpen: 'Open link', externalLinkCancel: 'Cancel', externalLinkOpening: 'Opening link…', externalLinkFailed: 'Could not open this link. You can retry or cancel.',
    conversationViewLabel: 'Conversation view', timelineView: 'Timeline', threadedView: 'Threaded Comments', collapseReplies: (count: number) => `Collapse ${count} replies on this page`, expandReplies: (count: number) => `Expand ${count} replies on this page`, replyDepth: (depth: number) => `Depth ${depth}`, viewPreferenceReadFailed: 'Conversation view preference could not be loaded.', viewPreferenceWriteFailed: 'Conversation view preference could not be saved. The selected view remains available in this session.',
    parentOutsidePage: (no: number) => `Reply to No. ${no} is on another page.`,
  },
  ja: {
    boards: 'ボード', accessible: 'アクセス可能なスレッド', refresh: '更新', loading: 'ワークスペースを読み込み中…', resizeBoards: 'ボード欄の幅を変更', resizeComposer: 'コンポーザー欄の高さを変更', quickStartTitle: 'Swarmへようこそ', quickStartIntro: '次の手順で会話を始めましょう。', quickStartStepOne: 'Tree ViewからBoardを選びます。', quickStartStepTwo: 'Threadを選択して会話を読みます。', quickStartStepThree: '権限があればツールバーからBoardまたはThreadを作成します。', closeThread: 'スレッドを閉じる', pageTop: 'ページトップ', pageBottom: 'ページボトム', restoringPosition: 'Threadの表示位置を復元しています…', restorePositionFailed: 'Threadの表示位置を復元できません。保存したページを表示しています。', scrollStorageReadFailed: 'Threadの表示位置を読み込めません。', scrollStorageWriteFailed: 'Threadの表示位置を保存できません。現在の位置はこのセッション中に保持されます。', jumpFailed: 'Threadの表示位置を復元できません。保存したページを表示しています。', layoutLoadFailed: 'レイアウト設定を読み込めません。初期サイズを使用します。', layoutSaveFailed: 'レイアウト設定を保存できません。現在のサイズはこのセッション中に保持されます。',
    unavailable: 'ワークスペースを読み込めませんでした。更新してください。', noBoards: '利用できるボードはありません。',
    checkingPermissions: '権限を確認中…', permissionFailed: '権限を確認できません。更新するまで作成操作はできません。', noCreatePermission: 'このThreadは閲覧できますが、Post・Responseの作成は許可されていません。', noBoardThreadPermission: 'Board・Threadの作成は許可されていません。',
    noThreads: 'このボードにスレッドはありません。', emptyThreads: 'アクセス可能なスレッドはありません。',
    select: 'スレッドを選択して会話を表示します。', threadLoading: 'スレッドを読み込み中…',
    threadFailed: 'スレッドを読み込めませんでした。再選択するか更新してください。', noPosts: '投稿はありません。',
    older: '過去の投稿を読み込む', loadingOlder: '過去の投稿を読み込み中…', responses: '件の直接返信',
    noResponses: '直接返信はありません。', responseFailed: '直接返信を読み込めませんでした。',
    replyTo: '返信先', deleted: '削除済み投稿', edited: '編集済み', unknown: '不明', olderFailed: '過去の投稿を読み込めませんでした。再試行してください。',
    edit: '編集', history: '履歴', editPost: '投稿を編集', cancel: 'キャンセル', saveEdit: '変更を保存', savingEdit: '保存中…',
    editBody: '投稿本文', editConflict: 'この投稿は編集画面を開いた後に更新されています。最新版を再読み込みして内容を確認してから保存してください。下書きは保持されています。',
    editCommunication: '編集要求の結果を確認できません。下書きは保持されています。再送する前に最新版を確認してください。', editFailed: '投稿を保存できませんでした。下書きは保持されています。',
    revisionInvalid: '現在の版番号を確認できません。ワークスペースを更新してから編集してください。',
    reloadLatest: '最新版を再読み込み', reloadingLatest: '再読み込み中…', latestVersion: '最新の保存済み本文', draft: '下書き',
    editSaved: '変更を保存しました。', historyTitle: '投稿の履歴', closeHistory: '履歴を閉じる', currentVersion: '現在の本文',
    beforeRevision: (revision: number) => `Revision ${revision} に置き換わる前の本文`, updatedBy: '更新者', savedAt: '保存日時',
    historyEmpty: 'この投稿に過去の版はありません。', historyFailed: '投稿の履歴を読み込めませんでした。', historyLoading: '履歴を読み込み中…',
    delete: '削除', restore: '復元', deletedItems: '削除済み項目', deletedItemsTitle: '削除済みBoard・Thread', closeDeletedItems: '削除済み項目を閉じる', deletedItemsLoading: '削除済み項目を読み込み中…', deletedItemsFailed: '削除済み項目を読み込めませんでした。', noDeletedItems: '復元できるBoard・Threadはありません。', deletedBoards: 'Board', deletedThreads: 'Thread', deletedAt: '削除日時',
    permanentlyDelete: '完全削除', selectDeleted: '選択', finishSelecting: '完了', selectAllDeleted: 'すべて選択', clearDeletedSelection: '選択を解除', selectedDeletedCount: (count: number) => `${count}件を選択中`, bulkRestore: '選択項目を復元', bulkPurge: '選択項目を完全削除', confirmBulkRestoreTitle: '復元の確認', confirmPurgeTitle: '完全削除の確認', bulkRestoreImpact: '一覧にある項目だけを復元します。選択したBoardとThreadはまとめて復元されます。ほかの削除済みThreadは削除状態のままです。', purgeBoardImpact: 'このBoardと配下のThread・Post・編集履歴・会話状態を完全に削除します。元に戻せません。', purgeThreadImpact: 'このThreadのPost・編集履歴・会話状態を完全に削除します。元に戻せません。', bulkPurgeImpact: '一覧にある対象だけを完全削除します。Boardを指定すると配下のThreadとPostも削除されます。', parentMustBeSelected: 'Threadと削除済みの親Boardを選択すると、まとめて復元できます。', bulkOperationFailed: '選択項目を変更できませんでした。一覧を更新しました。現在の項目と選択内容を確認してください。', staleSelection: '更新後の一覧にない選択項目があります。選択を解除し、現在の一覧を確認してください。',
    confirmDeleteTitle: '削除の確認', confirmRestoreTitle: '復元の確認', cancelManagement: 'キャンセル', confirmManagement: '実行', boardDeleteImpact: 'このBoardと配下のThread・Postを非表示にします。Boardを復元すると表示されます。個別に削除済みのThread・Postは削除状態のままです。', threadDeleteImpact: 'このThreadと配下のPostを非表示にします。Threadを復元すると表示されます。個別に削除済みのPostは削除状態のままです。', postDeleteImpact: 'Post本文を非表示にします。番号と削除済み表示は残ります。', restoreImpact: 'この項目だけを復元します。個別に削除済みの子項目は削除状態のままです。',
    managementFailed: '結果を確認できません。ワークスペースを更新しました。次の操作の前に現在の状態を確認してください。', managementRefreshing: '次の管理操作の前にワークスペースを更新してください。',
    createBoard: 'ボードを作成', createThread: 'スレッドを作成', newBoard: '新しいボード', newThread: '新しいスレッド', closeCreation: '閉じる', settings: '設定', search: '検索', searchNavigationFailed: 'この検索結果は利用できないか、開けません。ワークスペースを更新して再試行してください。',
    opBoard: 'ボード作成', opThread: 'スレッド作成', opPost: '投稿作成', opResponse: '返信作成',
    boardName: 'ボード名', threadName: 'スレッド名', description: '説明', parentBoard: 'ボード', chooseBoard: 'ボードを選択',
    create: '作成', postPlaceholder: '投稿を書く…', responsePlaceholder: '返信を書く…', sendPost: '投稿', sendResponse: '返信',
    cancelReply: '返信を取り消す', bodyRequired: '投稿する内容を入力してください。', bodyTooLarge: '本文はUTF-8で1 MiB以内にしてください。',
    nameRequired: '名前を入力してください。', creationFailed: '作成できませんでした。投稿の下書きは保持されています。',
    ackPending: '項目は作成されました。確認処理が保留中です。下書きは保持されています。安全に完了するには再試行してください。',
    replyUnavailable: 'この投稿には返信できません。',
    creationPending: '作成要求の結果が未確定です。再試行すると作成済みの項目を取得する場合があります。',
    discardCaution: '既に完了している場合があります。破棄するのは再試行記録だけです。', creationExpired: 'この要求は7日を過ぎており自動再送できません。続けるには再試行記録を破棄してください。',
    retryCreate: '保存済み作成を再試行', discardCreate: '再試行記録を破棄', checkCreation: '作成状態を確認', creationUnavailable: '作成状態を取得できません。作成やログアウトの前に再確認してください。',
    created: '作成しました。', byteCount: (count: number) => `${count.toLocaleString()} / 1,048,576 バイト`,
    nameTooLong: (limit: number) => `名前は${limit}文字以内にしてください。`, bodyTooLong: '本文は100,000文字以内にしてください。',
    externalLinkTitle: '外部リンクを開きますか？', externalLinkExplanation: 'このリンクはSwarmの外部で開きます。', externalLinkOpen: 'リンクを開く', externalLinkCancel: 'キャンセル', externalLinkOpening: 'リンクを開いています…', externalLinkFailed: 'リンクを開けませんでした。再試行するか、キャンセルしてください。',
    conversationViewLabel: '会話の表示形式', timelineView: 'タイムライン', threadedView: 'スレッド表示', collapseReplies: (count: number) => `このページの返信${count}件を折りたたむ`, expandReplies: (count: number) => `このページの返信${count}件を展開`, replyDepth: (depth: number) => `返信階層 ${depth}`, viewPreferenceReadFailed: '表示形式を読み込めません。', viewPreferenceWriteFailed: '表示形式を保存できません。選択した表示はこのセッション中に保持されます。',
    parentOutsidePage: (no: number) => `返信先 No.${no} は別ページにあります。`,
  },
} as const

function creationLabel(operation: CreateOperation | null, language: Language) {
  if (!operation) return ''
  const labels = words[language]
  if (operation === 'board.create') return labels.opBoard
  if (operation === 'thread.create') return labels.opThread
  if (operation === 'response.create') return labels.opResponse
  return labels.opPost
}

function pageControlLabel(language: Language, kind: 'first' | 'previous' | 'next' | 'last' | 'size' | 'navigation') {
  if (language === 'ja') {
    if (kind === 'first') return '最初のページ'
    if (kind === 'previous') return '前のページ'
    if (kind === 'next') return '次のページ'
    if (kind === 'last') return '最後のページ'
    if (kind === 'size') return '1ページあたりの投稿数'
    return 'Threadのページ移動'
  }
  if (kind === 'first') return 'First page'
  if (kind === 'previous') return 'Previous page'
  if (kind === 'next') return 'Next page'
  if (kind === 'last') return 'Last page'
  if (kind === 'size') return 'Posts per page'
  return 'Thread pages'
}

function threadLimitLabel(language: Language) {
  return language === 'ja'
    ? 'このThreadは投稿上限の1,000件に達したため、新しいPostやResponseを作成できません。'
    : 'This Thread has reached the 1,000 Post limit. New Posts and Responses are disabled.'
}

function pendingThreadLimitLabel(language: Language) {
  return language === 'ja'
    ? '保留中のPostまたはResponseのThreadが投稿上限の1,000件に達しています。対象のThreadを選択して状態を確認してください。'
    : 'The Thread for this pending Post or Response has reached the 1,000 Post limit. Select that Thread to review its status.'
}

function workspaceTabLabel(language: Language, tab: WorkspaceTab) {
  if (tab === 'boards') return language === 'ja' ? 'ボード' : 'Boards'
  if (tab === 'pinned') return language === 'ja' ? 'ピン留め' : 'Pinned'
  if (tab === 'archive') return language === 'ja' ? '過去スレ' : 'Archive'
  return language === 'ja' ? '更新履歴' : 'Activity'
}

function pinThreadLabel(language: Language, pinned: boolean) {
  if (pinned) return language === 'ja' ? 'Threadのピン留めを解除' : 'Unpin Thread'
  return language === 'ja' ? 'Threadをピン留め' : 'Pin Thread'
}

function pinReadFailedLabel(language: Language) {
  return language === 'ja' ? 'ピン留めしたThreadを読み込めませんでした。もう一度お試しください。' : 'Could not load pinned Threads. Try again.'
}

function pinSaveFailedLabel(language: Language) {
  return language === 'ja' ? 'Threadのピン留めを保存できませんでした。もう一度お試しください。' : 'Could not save the Thread pin. Try again.'
}

function activityReadFailedLabel(language: Language) {
  return language === 'ja' ? '更新履歴を読み込めませんでした。もう一度お試しください。' : 'Could not load Activity. Try again.'
}

function activityLoadingLabel(language: Language) {
  return language === 'ja' ? '更新履歴を読み込み中…' : 'Loading Activity…'
}

function activityEmptyLabel(language: Language) {
  return language === 'ja' ? '更新履歴はありません。' : 'No Activity yet.'
}

function activityActionLabel(action: string, language: Language) {
  const english: Record<string, string> = {
    'board.create': 'created a Board', 'board.delete': 'deleted a Board', 'board.restore': 'restored a Board',
    'thread.create': 'created a Thread', 'thread.delete': 'deleted a Thread', 'thread.restore': 'restored a Thread',
    'post.create': 'posted', 'response.create': 'replied', 'post.edit': 'edited a Post', 'post.delete': 'deleted a Post', 'post.restore': 'restored a Post',
  }
  const japanese: Record<string, string> = {
    'board.create': 'Boardを作成', 'board.delete': 'Boardを削除', 'board.restore': 'Boardを復元',
    'thread.create': 'Threadを作成', 'thread.delete': 'Threadを削除', 'thread.restore': 'Threadを復元',
    'post.create': '投稿', 'response.create': '返信', 'post.edit': 'Postを編集', 'post.delete': 'Postを削除', 'post.restore': 'Postを復元',
  }
  const labels = language === 'ja' ? japanese : english
  return labels[action] || action.replace(/[._-]+/g, ' ')
}

function pageLoadFailedLabel(language: Language) {
  return language === 'ja' ? 'ページを読み込めませんでした。もう一度お試しください。' : 'Could not load this page. Try again.'
}

function pageLoadingLabel(language: Language) {
  return language === 'ja' ? 'ページを読み込み中…' : 'Loading page…'
}

function authorLabel(post: Post, language: Language) {
  const display = post.author?.display_id || words[language].unknown
  const known = (value?: string | null) => !value || ['不明', 'unknown', 'user'].includes(value.toLowerCase()) ? words[language].unknown : value
  const model = known(post.author?.model_name)
  const harness = known(post.author?.harness_name)
  return `No.${post.no} ID: ${display} (${model}: ${harness})`
}

function historyEditorLabel(item: PostHistoryItem, language: Language) {
  const display = item.editor_display_id || words[language].unknown
  const model = item.editor_model_name || words[language].unknown
  return model.toLocaleLowerCase() === 'user' ? `USER: ${display}` : `${model}: ${display}`
}

export default function Workspace({ language, accountId = 'test', onLogout, onSettings, onSessionExpired, busy, refreshToken = 0, serviceState = 'unknown', serviceError = '', onServiceRefresh, serviceBusy = false, availableUpdate = null, updateNotes = null }: { language: Language; accountId?: string; onLogout: () => void; onSettings?: (opener: HTMLButtonElement) => void; onSessionExpired?: () => void; busy: boolean; refreshToken?: number; serviceState?: 'unknown' | 'checking' | 'starting' | 'stopping' | 'running' | 'stopped' | 'not_running'; serviceError?: string; onServiceRefresh?: () => void; serviceBusy?: boolean; availableUpdate?: AvailableUpdate | null; updateNotes?: UpdateNotes | null }) {
  const operationStatuses = useOperationStatuses()
  const scopedRefreshRunning = useRef(false)
  const [logExpanded, setLogExpanded] = useState(false)
  const statusLogId = useId()
  const t = words[language]
  const [boards, setBoards] = useState<Board[]>([])
  const [threads, setThreads] = useState<Thread[]>([])
  const [treeState, setTreeState] = useState<PageState>('loading')
  const [capabilityState, setCapabilityState] = useState<CapabilityState>('loading')
  const [capabilities, setCapabilities] = useState<ConversationCapabilities | null>(null)
  const [selectedId, setSelectedId] = useState<string | null>(null)
  const [conversationView, setConversationView] = useState<ConversationView>('timeline')
  const [viewPreferenceLoaded, setViewPreferenceLoaded] = useState(false)
  const [viewPreferenceError, setViewPreferenceError] = useState<'read' | 'write' | null>(null)
  const [preferenceRetryToken, setPreferenceRetryToken] = useState(0)
  const [collapsedConversationPosts, setCollapsedConversationPosts] = useState<Set<number>>(() => new Set())
  const authorizationRefreshTokenRef = useRef(refreshToken)
  const [searchOpen, setSearchOpen] = useState(false)
  const [searchQuery, setSearchQuery] = useState('')
  const [pendingExternalUrl, setPendingExternalUrl] = useState<string | null>(null)
  const [externalLinkBusy, setExternalLinkBusy] = useState(false)
  const [externalLinkError, setExternalLinkError] = useState(false)
  const [searchNavigationError, setSearchNavigationError] = useState('')
  const [searchHighlight, setSearchHighlight] = useState<Set<string>>(() => new Set())
  const [notificationError, setNotificationError] = useState('')
  const [notificationRetryToken, setNotificationRetryToken] = useState(0)
  const [activeWorkspaceTab, setActiveWorkspaceTab] = useState<WorkspaceTab>('boards')
  const [pinnedThreads, setPinnedThreads] = useState<Thread[]>([])
  const [pinnedState, setPinnedState] = useState<PageState>('loading')
  const [pinBusyId, setPinBusyId] = useState<string | null>(null)
  const [pinError, setPinError] = useState('')
  const [activityItems, setActivityItems] = useState<ActivityItem[]>([])
  const [activityState, setActivityState] = useState<PageState>('ready')
  const [activityLoadingMore, setActivityLoadingMore] = useState(false)
  const [activityError, setActivityError] = useState(false)
  const [activityNextId, setActivityNextId] = useState<number | null>(null)
  const [treeWidth, setTreeWidth] = useState(280)
  const [composerHeight, setComposerHeight] = useState(230)
  const layoutPreferenceValueRef = useRef({ treeWidth, composerHeight, logExpanded })
  layoutPreferenceValueRef.current = { treeWidth, composerHeight, logExpanded }
  const conversationViewPreferenceValueRef = useRef(conversationView)
  conversationViewPreferenceValueRef.current = conversationView
  const [layoutPreferencesLoaded, setLayoutPreferencesLoaded] = useState(false)
  const [layoutStorageError, setLayoutStorageError] = useState<'read' | 'write' | null>(null)
  const [masterDetailWidth, setMasterDetailWidth] = useState(0)
  const [conversationHeight, setConversationHeight] = useState(0)
  const [composerMinimum, setComposerMinimum] = useState(COMPOSER_HEIGHT_MIN)
  const [threadScrollStatus, setThreadScrollStatus] = useState<'restoring' | 'failed' | null>(null)
  const [threadScrollStorageError, setThreadScrollStorageError] = useState<'read' | 'write' | null>(null)
  const [collapsedBoards, setCollapsedBoards] = useState<Set<string>>(() => new Set())
  const [currentThread, setCurrentThread] = useState<Thread | null>(null)
  const [posts, setPosts] = useState<Post[]>([])
  const [threadPage, setThreadPage] = useState<ThreadPageState>({ page: 1, pageSize: 50, totalPosts: 0, totalPages: 1 })
  const threadPageRef = useRef(threadPage)
  const conversationRows = useMemo(() => buildConversationTree(posts), [posts])
  const visibleConversation = useMemo(() => visibleConversationRows(conversationRows, collapsedConversationPosts), [conversationRows, collapsedConversationPosts])
  const timelineRows = useMemo(() => posts.map((post) => ({ post, parentNo: null, depth: 0, descendantCount: 0, parentOutsidePage: false })), [posts])
  const displayedConversationRows = conversationView === 'threaded' ? visibleConversation : timelineRows
  const pendingConversationJump = useRef<{ threadId: string; postNo: number; detailGeneration: number; pageGeneration: number } | null>(null)
  const [detailState, setDetailState] = useState<PageState>('ready')
  const [paging, setPaging] = useState(false)
  const [pagingError, setPagingError] = useState(false)
  const [responses, setResponses] = useState<{ target: number; items: Post[] } | null>(null)
  const [responseState, setResponseState] = useState<'loading' | 'ready' | 'failed' | null>(null)
  const [responseNext, setResponseNext] = useState<number | null>(null)
  const [responsePaging, setResponsePaging] = useState(false)
  const [parentPost, setParentPost] = useState<Post | null>(null)
  const [postEditor, setPostEditor] = useState<PostEditState | null>(null)
  const [postHistory, setPostHistory] = useState<PostHistoryState | null>(null)
  const [postEditBusy, setPostEditBusy] = useState(false)
  const [deletedItems, setDeletedItems] = useState<DeletedConversations | null>(null)
  const [deletedState, setDeletedState] = useState<PageState>('ready')
  const [deletedOpen, setDeletedOpen] = useState(false)
  const [deletedSelectionMode, setDeletedSelectionMode] = useState(false)
  const [selectedDeletedKeys, setSelectedDeletedKeys] = useState<Set<string>>(() => new Set())
  const [bulkManagementAction, setBulkManagementAction] = useState<DeletedBulkAction | null>(null)
  const [managementTarget, setManagementTarget] = useState<ManagementTarget | null>(null)
  const [managementBusy, setManagementBusy] = useState(false)
  const [managementError, setManagementError] = useState('')
  const [managementNeedsRefresh, setManagementNeedsRefresh] = useState(false)
  const [creationStatus, setCreationStatus] = useState<CreationStatus>(emptyCreationStatus)
  const [creationCheck, setCreationCheck] = useState<PageState>('loading')
  const [creationBusy, setCreationBusy] = useState(false)
  const [creationError, setCreationError] = useState('')
  const [creationNotice, setCreationNotice] = useState('')
  const [boardName, setBoardName] = useState('')
  const [boardDescription, setBoardDescription] = useState('')
  const [threadName, setThreadName] = useState('')
  const [threadDescription, setThreadDescription] = useState('')
  const [threadBoardId, setThreadBoardId] = useState('')
  const [replyTarget, setReplyTarget] = useState<{ threadId: string; no: number } | null>(null)
  const [drafts, setDrafts] = useState<Record<string, string>>({})
  const [formError, setFormError] = useState('')
  const [boardFormError, setBoardFormError] = useState('')
  const [threadFormError, setThreadFormError] = useState('')
  const [creationDialog, setCreationDialog] = useState<'board' | 'thread' | null>(null)
  useEffect(() => registerUpdateBlocker(() =>
    Object.values(drafts).some(draft => draft.length > 0) || postEditor !== null || creationDialog !== null || creationBusy || postEditBusy
  ), [drafts, postEditor, creationDialog, creationBusy, postEditBusy])
  const treeRequest = useRef(0)
  const capabilityRequest = useRef(0)
  const detailRequest = useRef(0)
  const pageRequest = useRef(0)
  const relationRequest = useRef(0)
  const editRequest = useRef(0)
  const historyRequest = useRef(0)
  const deletedRequest = useRef(0)
  const pinsRequest = useRef(0)
  const pinMutationRequest = useRef(0)
  const pinMutationRunning = useRef(false)
  const activityRequest = useRef(0)
  const activityLoadingMoreRef = useRef(false)
  const activeWorkspaceTabRef = useRef(activeWorkspaceTab)
  const workspaceTabsRef = useRef<Partial<Record<WorkspaceTab, HTMLButtonElement | null>>>({})
  const masterDetailElement = useRef<HTMLDivElement | null>(null)
  const conversationElement = useRef<HTMLDivElement | null>(null)
  const threadScrollElement = useRef<HTMLElement | null>(null)
  const conversationNotifications = useConversationNotifications(accountId, refreshToken)
  const unreadCount = (id: string) => conversationNotifications.ready ? conversationNotifications.unread.items.find((item) => item.thread_id === id)?.unread_count || 0 : 0
  const unreadDot = (count: number) => count > 0 ? <span className="unread-dot" role="img" aria-label={language === 'ja' ? `未読${count}件` : `${count} unread posts`} title={language === 'ja' ? `未読${count}件` : `${count} unread posts`} /> : null
  const unreadBadge = (id: string) => <UnreadBadge language={language} count={unreadCount(id)} />
  const displayedRead = useDisplayedPostRead(accountId, currentThread?.id || null, threadScrollElement,
    `${pageRequest.current}:${conversationView}:${displayedConversationRows.map(({ post }) => post.id).join(',')}:${refreshToken}`,
    detailState === 'ready' && capabilityState === 'ready' && !paging && !busy && !creationBusy && !creationStatus.pending && !postEditBusy && threadScrollStatus !== 'restoring',
    conversationNotifications.refresh)
  const scrollCommit = useRef<ScrollCommit | null>(null)
  const suppressThreadScrollSave = useRef(false)
  const programmaticScrollTop = useRef<number | null>(null)
  const threadScrollSession = useRef(new Map<string, ThreadScrollPreference>())
  const treeDragStart = useRef<{ pointerId: number; clientX: number; width: number } | null>(null)
  const composerDragStart = useRef<{ pointerId: number; clientY: number; height: number } | null>(null)
  const selectedThreadButton = useRef<HTMLButtonElement | null>(null)
  const boardToggleRefs = useRef<Record<string, HTMLButtonElement | null>>({})
  const searchOpener = useRef<HTMLButtonElement | null>(null)
  const externalLinkOpener = useRef<HTMLButtonElement | null>(null)
  const externalLinkDialog = useRef<HTMLElement | null>(null)
  const externalLinkConfirmButton = useRef<HTMLButtonElement | null>(null)
  const externalLinkWasOpen = useRef(false)
  const externalLinkRequest = useRef(0)
  const externalLinkBusyRef = useRef(false)
  const externalLinkMounted = useRef(false)
  const externalLinkAccount = useRef(accountId)
  const searchNavigationRequest = useRef(0)
  const managementRunning = useRef(false)
  const postEditRunning = useRef(false)
  const historyOpener = useRef<HTMLButtonElement | null>(null)
  const historyDialog = useRef<HTMLElement | null>(null)
  const historyCloseButton = useRef<HTMLButtonElement | null>(null)
  const historyWasOpen = useRef(false)
  const managementOpener = useRef<HTMLButtonElement | null>(null)
  const managementDialog = useRef<HTMLElement | null>(null)
  const managementConfirmButton = useRef<HTMLButtonElement | null>(null)
  const managementWasOpen = useRef(false)
  const creationDialogRef = useRef<'board' | 'thread' | null>(null)
  const creationDialogSession = useRef(0)
  const creationDialogOpener = useRef<HTMLButtonElement | null>(null)
  const creationDialogElement = useRef<HTMLElement | null>(null)
  const creationDialogFirstField = useRef<HTMLInputElement | HTMLSelectElement | null>(null)
  const creationDialogWasOpen = useRef(false)
  const deletedOpener = useRef<HTMLButtonElement | null>(null)
  const deletedDialog = useRef<HTMLElement | null>(null)
  const deletedCloseButton = useRef<HTMLButtonElement | null>(null)
  const deletedWasOpen = useRef(false)
  const creationRunning = useRef(false)
  const creationStatusRef = useRef(creationStatus)
  const receiptRef = useRef<DraftReceipt | null>(null)
  const selectedIdRef = useRef(selectedId)
  const accountIdRef = useRef(accountId)
  accountIdRef.current = accountId
  const layoutPreferenceGenerationRef = useRef(0)
  const viewPreferenceGenerationRef = useRef(0)
  const layoutEditSequenceRef = useRef(0)
  const viewEditSequenceRef = useRef(0)
  const layoutLastPersistedRef = useRef<string | null>(null)
  const viewLastPersistedRef = useRef<ConversationView | null>(null)
  const layoutLoadedAccountRef = useRef<string | null>(null)
  const viewLoadedAccountRef = useRef<string | null>(null)
  const layoutRequestedAccountRef = useRef<string | null>(null)
  const viewRequestedAccountRef = useRef<string | null>(null)
  const layoutReadFailureBaselineRef = useRef<number | null>(null)
  const viewReadFailureBaselineRef = useRef<number | null>(null)
  const threadScrollReadFailureBaselineRef = useRef(new Map<string, number>())
  const threadScrollEditSequenceRef = useRef(new Map<string, number>())
  const preferenceTimersRef = useRef(new Map<string, ReturnType<typeof setTimeout>>())
  const preferenceSequencesRef = useRef(new Map<string, number>())
  const preferenceWriteQueuesRef = useRef(new Map<string, Promise<void>>())
  const pendingPreferenceWritesRef = useRef(new Map<string, { accountId: string; run: () => Promise<void>; enqueue: () => Promise<void> }>())
  const pendingPreferenceReadsRef = useRef(new Set<Promise<unknown>>())
  const postsRef = useRef(posts)
  const currentThreadRef = useRef(currentThread)
  selectedIdRef.current = selectedId
  activeWorkspaceTabRef.current = activeWorkspaceTab
  postsRef.current = posts
  currentThreadRef.current = currentThread
  threadPageRef.current = threadPage
  creationStatusRef.current = creationStatus

  const trackPreferenceRead = <T,>(promise: Promise<T>): Promise<T> => {
    pendingPreferenceReadsRef.current.add(promise)
    void promise.then(() => pendingPreferenceReadsRef.current.delete(promise), () => pendingPreferenceReadsRef.current.delete(promise))
    return promise
  }

  const scheduleUiPreferenceWrite = (
    scope: UiPreferenceScope,
    key: string | undefined,
    legacyStorageKey: string,
    value: unknown,
    normalize: (value: unknown) => unknown,
    setError: PreferenceErrorSetter,
    accountIdOverride?: string,
  ) => {
    const writeAccountId = accountIdOverride ?? accountId
    const writeKey = `${writeAccountId}:${scope}:${key || ''}`
    const sequence = (preferenceSequencesRef.current.get(writeKey) || 0) + 1
    preferenceSequencesRef.current.set(writeKey, sequence)
    const complete = (error: PreferenceError) => {
      if (accountIdRef.current === writeAccountId && preferenceSequencesRef.current.get(writeKey) === sequence) setError(error)
    }
    const save = () => saveUiPreference({ accountId: writeAccountId, scope, key, legacyStorageKey, value, normalize })
    if (!isDesktopPreferenceHost()) {
      void save().then(() => complete(null)).catch(() => complete('write'))
      return
    }

    const pending = preferenceTimersRef.current.get(writeKey)
    if (pending) clearTimeout(pending)
    const item: { accountId: string; run: () => Promise<void>; enqueue: () => Promise<void>; inflight: Promise<void> | null } = { accountId: writeAccountId, inflight: null, run: async () => {
      if (accountIdRef.current !== writeAccountId) throw new Error('Preference account changed before save.')
      await save()
      // Record each acknowledged write in queue order. An older queued write may
      // complete before the latest one, in which case the newer acknowledgment
      // will replace this baseline with the final DB value.
      if (accountIdRef.current === writeAccountId) {
        const normalized = normalize(value)
        if (scope === 'layout') layoutLastPersistedRef.current = JSON.stringify(normalized)
        else if (scope === 'conversation_view') viewLastPersistedRef.current = normalizeConversationView(normalized)
      }
      complete(null)
      if (pendingPreferenceWritesRef.current.get(writeKey) === item) pendingPreferenceWritesRef.current.delete(writeKey)
    }, enqueue: () => Promise.resolve() }
    item.enqueue = () => {
      if (item.inflight) return item.inflight
      const previous = preferenceWriteQueuesRef.current.get(writeKey) || Promise.resolve()
      const operation = previous.catch(() => undefined).then(async () => {
        await item.run()
      }).catch(() => complete('write'))
      item.inflight = operation
      preferenceWriteQueuesRef.current.set(writeKey, operation)
      void operation.finally(() => {
        if (preferenceWriteQueuesRef.current.get(writeKey) === operation) preferenceWriteQueuesRef.current.delete(writeKey)
        if (item.inflight === operation) item.inflight = null
      })
      return operation
    }
    pendingPreferenceWritesRef.current.set(writeKey, item)
    const timer = setTimeout(() => {
      preferenceTimersRef.current.delete(writeKey)
      void item.enqueue()
    }, 180)
    preferenceTimersRef.current.set(writeKey, timer)
  }

  useEffect(() => {
    if (!isDesktopPreferenceHost()) return
    return registerPreferenceFlush(async () => {
      // Stop every pending debounce immediately on receipt of native close.
      for (const timer of preferenceTimersRef.current.values()) clearTimeout(timer)
      preferenceTimersRef.current.clear()
      await Promise.all([...pendingPreferenceReadsRef.current])
      await Promise.resolve()
      if (layoutReadFailureBaselineRef.current !== null && layoutEditSequenceRef.current !== layoutReadFailureBaselineRef.current) throw new Error('Layout edits have no confirmed persisted baseline.')
      if (viewReadFailureBaselineRef.current !== null && viewEditSequenceRef.current !== viewReadFailureBaselineRef.current) throw new Error('Conversation view edits have no confirmed persisted baseline.')
      for (const [key, baseline] of threadScrollReadFailureBaselineRef.current) {
        if ((threadScrollEditSequenceRef.current.get(key) || 0) !== baseline) throw new Error('Thread position edits have no confirmed persisted baseline.')
      }
      const activeAccountId = accountIdRef.current
      if (layoutLoadedAccountRef.current === activeAccountId) {
        const layout = normalizeLayoutPreferences({ version: 1, ...layoutPreferenceValueRef.current })
        if (JSON.stringify(layout) !== layoutLastPersistedRef.current) {
          scheduleUiPreferenceWrite('layout', undefined, layoutPreferencesKey(activeAccountId), layout, normalizeLayoutPreferences, setLayoutStorageError, activeAccountId)
        }
      }
      if (viewLoadedAccountRef.current === activeAccountId && conversationViewPreferenceValueRef.current !== viewLastPersistedRef.current) {
        const view = conversationViewPreferenceValueRef.current
        scheduleUiPreferenceWrite('conversation_view', undefined, conversationViewPreferenceKey(activeAccountId), view, normalizeConversationView, setViewPreferenceError, activeAccountId)
      }
      const pending = [...pendingPreferenceWritesRef.current.entries()]
      for (const [key, item] of pending) {
        const timer = preferenceTimersRef.current.get(key)
        if (timer) { clearTimeout(timer); preferenceTimersRef.current.delete(key) }
        await item.enqueue()
      }
      await Promise.all([...preferenceWriteQueuesRef.current.values()])
      if (pendingPreferenceWritesRef.current.size > 0) throw new Error('One or more UI preference writes remain unconfirmed.')
    })
  }, [])

  const threadScrollVisibleTop = (container: HTMLElement) => {
    const containerRect = container.getBoundingClientRect()
    const headingRect = container.querySelector<HTMLElement>('.thread-heading')?.getBoundingClientRect()
    return headingRect && headingRect.bottom > containerRect.top
      ? Math.min(headingRect.bottom, containerRect.bottom)
      : containerRect.top
  }

  const readCurrentThreadScroll = (): ThreadScrollPreference | null => {
    const container = threadScrollElement.current
    if (!container) return null
    const visibleTop = threadScrollVisibleTop(container)
    const visiblePost = Array.from(container.querySelectorAll<HTMLElement>('.timeline-post')).find((post) => post.getBoundingClientRect().bottom > visibleTop)
    const postNo = visiblePost ? Number(visiblePost.dataset.postNo) : null
    return {
      version: 1,
      postNo: postNo !== null && Number.isSafeInteger(postNo) && postNo > 0 ? postNo : null,
      offset: visiblePost ? visiblePost.getBoundingClientRect().top - visibleTop : 0,
      scrollTop: Math.max(0, container.scrollTop),
      atBottom: container.scrollHeight > container.clientHeight && container.scrollHeight - container.clientHeight - container.scrollTop <= 24,
      page: threadPageRef.current.page,
      pageSize: threadPageRef.current.pageSize,
    }
  }

  const saveThreadScrollPreference = (threadId: string, preference: ThreadScrollPreference, reportError = true) => {
    const key = threadScrollPreferenceKey(accountId, threadId)
    const normalized = normalizeThreadScrollPreference(preference)
    threadScrollSession.current.set(key, normalized)
    const editSequence = (threadScrollEditSequenceRef.current.get(key) || 0) + 1
    threadScrollEditSequenceRef.current.set(key, editSequence)
    // After an unreadable DB result, keep changes in memory until a retry establishes
    // the persisted value. Never overwrite a possibly restored preference blindly.
    if (threadScrollReadFailureBaselineRef.current.has(key)) return
    scheduleUiPreferenceWrite('thread_scroll', threadId, key, normalized, normalizeThreadScrollPreference,
      reportError ? setThreadScrollStorageError : () => undefined)
  }

  const loadThreadScrollPreference = async (threadId: string, request: number): Promise<ThreadScrollPreference> => {
    const key = threadScrollPreferenceKey(accountId, threadId)
    const cached = threadScrollSession.current.get(key)
    const failureBaseline = threadScrollReadFailureBaselineRef.current.get(key)
    if (cached && failureBaseline === undefined) return cached
    const editsAtStart = threadScrollEditSequenceRef.current.get(key) || 0
    const result = await trackPreferenceRead(loadUiPreference({
      accountId,
      scope: 'thread_scroll',
      key: threadId,
      legacyStorageKey: key,
      defaultValue: DEFAULT_THREAD_SCROLL_PREFERENCE,
      normalize: normalizeThreadScrollPreference,
      parseLegacy: parseThreadScrollPreference,
      isCurrent: () => accountIdRef.current === accountId && request === detailRequest.current,
    }))
    if (accountIdRef.current !== accountId || request !== detailRequest.current) return { ...DEFAULT_THREAD_SCROLL_PREFERENCE }
    if (result.error === 'read') {
      if (failureBaseline === undefined) threadScrollReadFailureBaselineRef.current.set(key, editsAtStart)
      threadScrollSession.current.set(key, cached || result.value)
      setThreadScrollStorageError('read')
      return cached || result.value
    }
    const baseline = threadScrollReadFailureBaselineRef.current.get(key)
    const hasEditsAfterFailure = baseline !== undefined && (threadScrollEditSequenceRef.current.get(key) || 0) !== baseline
    const value = hasEditsAfterFailure ? (threadScrollSession.current.get(key) || result.value) : result.value
    threadScrollReadFailureBaselineRef.current.delete(key)
    threadScrollSession.current.set(key, value)
    setThreadScrollStorageError(result.error)
    if (hasEditsAfterFailure) scheduleUiPreferenceWrite('thread_scroll', threadId, key, value, normalizeThreadScrollPreference, setThreadScrollStorageError)
    return value
  }

  const persistThreadScroll = (threadId: string, reportError = true) => {
    if (selectedIdRef.current !== threadId || suppressThreadScrollSave.current) return
    const preference = readCurrentThreadScroll()
    if (preference) saveThreadScrollPreference(threadId, preference, reportError)
  }

  const handleThreadScroll = () => {
    if (suppressThreadScrollSave.current) return
    const threadId = currentThreadRef.current?.id
    if (!threadId || selectedIdRef.current !== threadId) return
    const container = threadScrollElement.current
    if (container && programmaticScrollTop.current !== null) {
      if (container.scrollTop === programmaticScrollTop.current) {
        programmaticScrollTop.current = null
        return
      }
      programmaticScrollTop.current = null
    }
    persistThreadScroll(threadId)
  }

  const jumpToPageEdge = (edge: 'top' | 'bottom') => {
    const container = threadScrollElement.current
    const threadId = currentThreadRef.current?.id
    if (!container || !threadId || selectedIdRef.current !== threadId) return
    container.scrollTop = edge === 'top' ? 0 : Math.max(0, container.scrollHeight - container.clientHeight)
    programmaticScrollTop.current = container.scrollTop
    persistThreadScroll(threadId)
  }

  const treeWidthMaximum = Math.max(220, Math.min(480, (masterDetailWidth || 808) - 328))
  const boundedTreeWidth = Math.min(treeWidth, treeWidthMaximum)
  const composerHeightMaximum = Math.max(composerMinimum, Math.min(480, (conversationHeight || 608) - 128))
  const boundedComposerHeight = Math.max(composerMinimum, Math.min(composerHeight, composerHeightMaximum))
  const resizeTree = (width: number) => {
    layoutEditSequenceRef.current += 1
    setTreeWidth(Math.max(220, Math.min(treeWidthMaximum, width)))
  }
  const resizeComposer = (height: number) => {
    layoutEditSequenceRef.current += 1
    setComposerHeight(Math.max(composerMinimum, Math.min(composerHeightMaximum, height)))
  }

  const handleWorkspaceTabKeyDown = (event: ReactKeyboardEvent<HTMLButtonElement>) => {
    const tabs: WorkspaceTab[] = ['boards', 'pinned', 'archive', 'activity']
    const index = tabs.indexOf(event.currentTarget.dataset.workspaceTab as WorkspaceTab)
    let nextIndex = index
    if (event.key === 'ArrowRight') nextIndex = (index + 1) % tabs.length
    else if (event.key === 'ArrowLeft') nextIndex = (index + tabs.length - 1) % tabs.length
    else if (event.key === 'Home') nextIndex = 0
    else if (event.key === 'End') nextIndex = tabs.length - 1
    else return
    event.preventDefault()
    const next = tabs[nextIndex]
    setActiveWorkspaceTab(next)
    workspaceTabsRef.current[next]?.focus()
  }

  const handleTreeResizeKeyDown = (event: ReactKeyboardEvent<HTMLDivElement>) => {
    if (event.key === 'ArrowLeft') { event.preventDefault(); resizeTree(boundedTreeWidth - 16) }
    else if (event.key === 'ArrowRight') { event.preventDefault(); resizeTree(boundedTreeWidth + 16) }
    else if (event.key === 'Home') { event.preventDefault(); resizeTree(220) }
    else if (event.key === 'End') { event.preventDefault(); resizeTree(treeWidthMaximum) }
  }

  const handleTreeResizePointerDown = (event: ReactPointerEvent<HTMLDivElement>) => {
    if (event.pointerType === 'mouse' && event.button !== 0) return
    treeDragStart.current = { pointerId: event.pointerId, clientX: event.clientX, width: boundedTreeWidth }
    event.currentTarget.setPointerCapture(event.pointerId)
  }

  const handleTreeResizePointerMove = (event: ReactPointerEvent<HTMLDivElement>) => {
    const drag = treeDragStart.current
    if (event.currentTarget.hasPointerCapture(event.pointerId) && drag?.pointerId === event.pointerId) resizeTree(drag.width + event.clientX - drag.clientX)
  }

  const endTreeResize = (event: ReactPointerEvent<HTMLDivElement>) => {
    if (treeDragStart.current?.pointerId === event.pointerId) treeDragStart.current = null
  }

  const handleComposerResizeKeyDown = (event: ReactKeyboardEvent<HTMLDivElement>) => {
    if (event.key === 'ArrowUp') { event.preventDefault(); resizeComposer(boundedComposerHeight + 16) }
    else if (event.key === 'ArrowDown') { event.preventDefault(); resizeComposer(boundedComposerHeight - 16) }
    else if (event.key === 'Home') { event.preventDefault(); resizeComposer(composerMinimum) }
    else if (event.key === 'End') { event.preventDefault(); resizeComposer(composerHeightMaximum) }
  }

  const handleComposerResizePointerDown = (event: ReactPointerEvent<HTMLDivElement>) => {
    if (event.pointerType === 'mouse' && event.button !== 0) return
    composerDragStart.current = { pointerId: event.pointerId, clientY: event.clientY, height: boundedComposerHeight }
    event.currentTarget.setPointerCapture(event.pointerId)
  }

  const handleComposerResizePointerMove = (event: ReactPointerEvent<HTMLDivElement>) => {
    const drag = composerDragStart.current
    if (event.currentTarget.hasPointerCapture(event.pointerId) && drag?.pointerId === event.pointerId) resizeComposer(drag.height + drag.clientY - event.clientY)
  }

  const endComposerResize = (event: ReactPointerEvent<HTMLDivElement>) => {
    if (composerDragStart.current?.pointerId === event.pointerId) composerDragStart.current = null
  }

  const closeThread = () => {
    ++searchNavigationRequest.current
    setSearchNavigationError('')
    pendingConversationJump.current = null
    setCollapsedConversationPosts(new Set())
    const openThreadId = currentThreadRef.current?.id
    if (openThreadId) persistThreadScroll(openThreadId)
    const focusTarget = selectedThreadButton.current
    ++detailRequest.current
    ++pageRequest.current
    ++relationRequest.current
    ++capabilityRequest.current
    selectedIdRef.current = null
    setSelectedId(null)
    setCurrentThread(null)
    setPosts([])
    setThreadPage({ page: 1, pageSize: 50, totalPosts: 0, totalPages: 1 })
    scrollCommit.current = null
    suppressThreadScrollSave.current = false
    setThreadScrollStatus(null)
    setPaging(false)
    setPagingError(false)
    setResponses(null)
    setResponseNext(null)
    setResponsePaging(false)
    setResponseState(null)
    setParentPost(null)
    setReplyTarget(null)
    const permissionRequest = ++capabilityRequest.current
    if (capabilityState === 'loading' || !capabilities) {
      setCapabilityState('loading')
      void guiRead('conversation.capabilities').then((result) => {
        if (permissionRequest !== capabilityRequest.current || selectedIdRef.current !== null) return
        setCapabilities(result)
        setCapabilityState('ready')
      }).catch((error: unknown) => {
        if (permissionRequest !== capabilityRequest.current || selectedIdRef.current !== null) return
        if (isAccessFault(error)) { clearPrivateWorkspace(); return }
        setCapabilities(null)
        setCapabilityState('failed')
      })
    }
    setDetailState('ready')
    closePostEditor()
    closePostHistory()
    queueMicrotask(() => focusTarget?.focus())
  }

  const closeSearchDialog = useCallback(() => setSearchOpen(false), [])

  const requestExternalOpen = useCallback((rawUrl: string, opener: HTMLButtonElement) => {
    const normalizedUrl = normalizeSafeExternalUrl(rawUrl)
    if (!normalizedUrl) return
    externalLinkOpener.current = opener
    opener.focus({ preventScroll: true })
    externalLinkBusyRef.current = false
    setExternalLinkBusy(false)
    setExternalLinkError(false)
    ++externalLinkRequest.current
    setPendingExternalUrl(normalizedUrl)
  }, [])

  const closeExternalLink = useCallback(() => {
    if (externalLinkBusyRef.current) return
    ++externalLinkRequest.current
    setPendingExternalUrl(null)
    setExternalLinkError(false)
  }, [])

  const confirmExternalLink = useCallback(async () => {
    const url = pendingExternalUrl
    if (!url || externalLinkBusyRef.current || busy || creationBusy || creationStatus.pending) return
    const request = ++externalLinkRequest.current
    externalLinkBusyRef.current = true
    setExternalLinkBusy(true)
    setExternalLinkError(false)
    try {
      await guiOpenLink(url)
      if (!externalLinkMounted.current || request !== externalLinkRequest.current) return
      setPendingExternalUrl(null)
      setExternalLinkError(false)
    } catch {
      if (!externalLinkMounted.current || request !== externalLinkRequest.current) return
      setExternalLinkError(true)
    } finally {
      if (externalLinkMounted.current && request === externalLinkRequest.current) {
        externalLinkBusyRef.current = false
        setExternalLinkBusy(false)
      }
    }
  }, [busy, creationBusy, creationStatus.pending, pendingExternalUrl])

  const handleExternalLinkKeyDown = (event: ReactKeyboardEvent<HTMLElement>) => {
    if (event.key === 'Escape') {
      event.preventDefault()
      closeExternalLink()
      return
    }
    if (event.key !== 'Tab') return
    const dialog = externalLinkDialog.current
    if (!dialog) return
    const focusable = Array.from(dialog.querySelectorAll<HTMLElement>('button:not([disabled]), a[href], input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])'))
    if (!focusable.length) { event.preventDefault(); dialog.focus(); return }
    const first = focusable[0]
    const last = focusable[focusable.length - 1]
    if (event.shiftKey && (document.activeElement === first || !dialog.contains(document.activeElement))) { event.preventDefault(); last.focus() }
    else if (!event.shiftKey && (document.activeElement === last || !dialog.contains(document.activeElement))) { event.preventDefault(); first.focus() }
  }

  useEffect(() => {
    externalLinkMounted.current = true
    return () => {
      externalLinkMounted.current = false
      externalLinkBusyRef.current = false
      ++externalLinkRequest.current
      externalLinkOpener.current = null
    }
  }, [])

  useEffect(() => {
    if (pendingExternalUrl) {
      externalLinkWasOpen.current = true
      externalLinkConfirmButton.current?.focus()
    } else if (externalLinkWasOpen.current) {
      externalLinkWasOpen.current = false
      const opener = externalLinkOpener.current
      externalLinkOpener.current = null
      if (opener?.isConnected) opener.focus()
    }
  }, [pendingExternalUrl])

  const historyDialogOpen = postHistory !== null
  const externalLinkDialogOpen = pendingExternalUrl !== null
  useLayoutEffect(() => {
    if (!historyDialogOpen && !externalLinkDialogOpen) return
    const rootOverflow = document.documentElement.style.overflow
    const bodyOverflow = document.body.style.overflow
    document.documentElement.style.overflow = 'hidden'
    document.body.style.overflow = 'hidden'
    return () => {
      document.documentElement.style.overflow = rootOverflow
      document.body.style.overflow = bodyOverflow
    }
  }, [externalLinkDialogOpen, historyDialogOpen])

  function openSearchDialog(opener: HTMLButtonElement) {
    searchOpener.current = opener
    setSearchNavigationError('')
    setSearchOpen(true)
  }

  useEffect(() => {
    if (treeState !== 'ready') return
    const element = masterDetailElement.current
    if (!element) return
    const measure = () => {
      setMasterDetailWidth(element.clientWidth)
      setConversationHeight(conversationElement.current?.clientHeight ?? 0)
    }
    measure()
    const observer = typeof ResizeObserver === 'undefined' ? null : new ResizeObserver(measure)
    observer?.observe(element)
    if (conversationElement.current) observer?.observe(conversationElement.current)
    window.addEventListener('resize', measure)
    return () => {
      observer?.disconnect()
      window.removeEventListener('resize', measure)
    }
  }, [treeState])

  useLayoutEffect(() => {
    const commit = scrollCommit.current
    if (!commit || detailState !== 'ready' || currentThread?.id !== commit.threadId) return
    if (commit.generation !== detailRequest.current || commit.pageGeneration !== pageRequest.current || selectedIdRef.current !== commit.threadId) {
      scrollCommit.current = null
      if (commit.pageGeneration === pageRequest.current) suppressThreadScrollSave.current = false
      return
    }
    const container = threadScrollElement.current
    if (!container) return
    const preference = commit.preference
    const target = preference.postNo === null ? null : container.querySelector<HTMLElement>(`#post-${preference.postNo}`)
    let desiredScrollTop: number
    let restoreFailed = commit.failed === true
    if (preference.atBottom) {
      desiredScrollTop = container.scrollHeight
    } else if (target) {
      desiredScrollTop = container.scrollTop + (target.getBoundingClientRect().top - threadScrollVisibleTop(container) - preference.offset)
    } else {
      desiredScrollTop = preference.scrollTop
      if (preference.postNo !== null) restoreFailed = true
    }
    scrollCommit.current = null
    suppressThreadScrollSave.current = true
    const maximumScrollTop = Math.max(0, container.scrollHeight - container.clientHeight)
    container.scrollTop = container.scrollHeight === 0 && container.clientHeight === 0
      ? Math.max(0, desiredScrollTop)
      : Math.max(0, Math.min(desiredScrollTop, maximumScrollTop))
    programmaticScrollTop.current = container.scrollTop
    setThreadScrollStatus(restoreFailed ? 'failed' : null)
    const finish = () => {
      if (detailRequest.current !== commit.generation || pageRequest.current !== commit.pageGeneration || selectedIdRef.current !== commit.threadId) return
      suppressThreadScrollSave.current = false
      if (!restoreFailed) {
        if (commit.preservePreference) saveThreadScrollPreference(commit.threadId, { ...preference, scrollTop: Math.max(0, container.scrollTop), atBottom: false }, false)
        else persistThreadScroll(commit.threadId, false)
      }
    }
    const settle = () => {
      if (detailRequest.current !== commit.generation || pageRequest.current !== commit.pageGeneration || selectedIdRef.current !== commit.threadId) return
      if (!restoreFailed && target?.isConnected && !preference.atBottom) {
        const actualOffset = target.getBoundingClientRect().top - threadScrollVisibleTop(container)
        const correction = actualOffset - preference.offset
        if (Math.abs(correction) > 0.5) {
          const maximum = Math.max(0, container.scrollHeight - container.clientHeight)
          container.scrollTop = Math.max(0, Math.min(container.scrollTop + correction, maximum))
          programmaticScrollTop.current = container.scrollTop
        }
      }
      if (typeof window.requestAnimationFrame === 'function') window.requestAnimationFrame(finish)
      else window.setTimeout(finish, 0)
    }
    if (typeof window.requestAnimationFrame === 'function') window.requestAnimationFrame(settle)
    else window.setTimeout(settle, 0)
  }, [collapsedConversationPosts, conversationView, currentThread, detailState, posts])

  useLayoutEffect(() => {
    const jump = pendingConversationJump.current
    if (!jump) return
    if (jump.detailGeneration !== detailRequest.current || jump.pageGeneration !== pageRequest.current || selectedIdRef.current !== jump.threadId || currentThread?.id !== jump.threadId) {
      pendingConversationJump.current = null
      return
    }
    const target = threadScrollElement.current?.querySelector<HTMLElement>(`#post-${jump.postNo}`)
    if (target) target.scrollIntoView?.({ behavior: 'smooth', block: 'center' })
    pendingConversationJump.current = null
  }, [collapsedConversationPosts, conversationView, currentThread, posts])

  useEffect(() => {
    const generation = ++viewPreferenceGenerationRef.current
    const editsAtStart = viewEditSequenceRef.current
    const accountChanged = viewRequestedAccountRef.current !== accountId
    viewRequestedAccountRef.current = accountId
    let active = true
    viewLoadedAccountRef.current = null
    setViewPreferenceLoaded(false)
    if (accountChanged) {
      pendingConversationJump.current = null
      setSearchHighlight(new Set())
      setSearchQuery('')
      setCollapsedConversationPosts(new Set())
      setConversationView('timeline')
      setViewPreferenceError(null)
      viewLastPersistedRef.current = null
      viewReadFailureBaselineRef.current = null
    }
    void trackPreferenceRead(loadUiPreference({
      accountId,
      scope: 'conversation_view',
      legacyStorageKey: conversationViewPreferenceKey(accountId),
      defaultValue: 'timeline' as ConversationView,
      normalize: normalizeConversationView,
      parseLegacy: parseConversationViewPreference,
      isCurrent: () => active && generation === viewPreferenceGenerationRef.current && accountIdRef.current === accountId,
    }).then((result) => {
      if (!active || generation !== viewPreferenceGenerationRef.current || accountIdRef.current !== accountId) return
      if (result.error === 'read') {
        if (viewReadFailureBaselineRef.current === null) viewReadFailureBaselineRef.current = editsAtStart
        setViewPreferenceError('read')
        setViewPreferenceLoaded(true)
        return
      }
      const baseline = viewReadFailureBaselineRef.current
      const editedAfterFailure = baseline !== null && viewEditSequenceRef.current !== baseline
      const editedDuringLoad = viewEditSequenceRef.current !== editsAtStart
      viewLastPersistedRef.current = result.value
      viewReadFailureBaselineRef.current = null
      viewLoadedAccountRef.current = accountId
      if (!editedAfterFailure && !editedDuringLoad) setConversationView(result.value)
      setViewPreferenceError(result.error)
      setViewPreferenceLoaded(true)
    }))
    return () => { active = false }
  }, [accountId, preferenceRetryToken])

  useEffect(() => {
    const generation = ++layoutPreferenceGenerationRef.current
    const editsAtStart = layoutEditSequenceRef.current
    const accountChanged = layoutRequestedAccountRef.current !== accountId
    layoutRequestedAccountRef.current = accountId
    let active = true
    layoutLoadedAccountRef.current = null
    setLayoutPreferencesLoaded(false)
    if (accountChanged) {
      setTreeWidth(DEFAULT_LAYOUT_PREFERENCES.treeWidth)
      setComposerHeight(DEFAULT_LAYOUT_PREFERENCES.composerHeight)
      setLogExpanded(false)
      setLayoutStorageError(null)
      layoutLastPersistedRef.current = null
      layoutReadFailureBaselineRef.current = null
    }
    void trackPreferenceRead(loadUiPreference({
      accountId,
      scope: 'layout',
      legacyStorageKey: layoutPreferencesKey(accountId),
      defaultValue: DEFAULT_LAYOUT_PREFERENCES,
      normalize: normalizeLayoutPreferences,
      parseLegacy: parseLayoutPreferences,
      isCurrent: () => active && generation === layoutPreferenceGenerationRef.current && accountIdRef.current === accountId,
    }).then((result) => {
      if (!active || generation !== layoutPreferenceGenerationRef.current || accountIdRef.current !== accountId) return
      if (result.error === 'read') {
        if (layoutReadFailureBaselineRef.current === null) layoutReadFailureBaselineRef.current = editsAtStart
        setLayoutStorageError('read')
        setLayoutPreferencesLoaded(true)
        return
      }
      const baseline = layoutReadFailureBaselineRef.current
      const editedAfterFailure = baseline !== null && layoutEditSequenceRef.current !== baseline
      const editedDuringLoad = layoutEditSequenceRef.current !== editsAtStart
      const preferences = normalizeLayoutPreferences(result.value)
      layoutLastPersistedRef.current = JSON.stringify(preferences)
      layoutReadFailureBaselineRef.current = null
      layoutLoadedAccountRef.current = accountId
      if (!editedAfterFailure && !editedDuringLoad) {
        setTreeWidth(preferences.treeWidth)
        setComposerHeight(preferences.composerHeight)
        setLogExpanded(preferences.logExpanded === true)
      }
      // If the user edited during the read, the normal save effect writes that
      // current state only now that the persisted DB state is known.
      setLayoutStorageError(result.error)
      setLayoutPreferencesLoaded(true)
    }))
    return () => { active = false }
  }, [accountId, preferenceRetryToken])

  useEffect(() => {
    if (!layoutPreferencesLoaded || layoutLoadedAccountRef.current !== accountId) return
    const preferences = normalizeLayoutPreferences({ version: 1, treeWidth, composerHeight, logExpanded })
    const serialized = JSON.stringify(preferences)
    if (serialized === layoutLastPersistedRef.current) return
    scheduleUiPreferenceWrite('layout', undefined, layoutPreferencesKey(accountId), preferences, normalizeLayoutPreferences, setLayoutStorageError)
  }, [accountId, composerHeight, layoutPreferencesLoaded, treeWidth, logExpanded])

  useEffect(() => {
    if (!viewPreferenceLoaded || viewLoadedAccountRef.current !== accountId || conversationView === viewLastPersistedRef.current) return
    scheduleUiPreferenceWrite('conversation_view', undefined, conversationViewPreferenceKey(accountId), conversationView, normalizeConversationView, setViewPreferenceError)
  }, [accountId, conversationView, viewPreferenceLoaded])

  useLayoutEffect(() => {
    if (externalLinkAccount.current !== accountId) {
      externalLinkAccount.current = accountId
      ++externalLinkRequest.current
      externalLinkBusyRef.current = false
      externalLinkOpener.current = null
      setPendingExternalUrl(null)
      setExternalLinkBusy(false)
      setExternalLinkError(false)
    }
    return () => {
      const openThreadId = currentThreadRef.current?.id
      if (openThreadId) persistThreadScroll(openThreadId, false)
    }
  }, [accountId])

  const closePostEditor = useCallback(() => {
    ++editRequest.current
    setPostEditor(null)
  }, [])

  const closePostHistory = useCallback(() => {
    ++historyRequest.current
    setPostHistory(null)
  }, [])

  const closeCreationDialog = useCallback((restoreFocus = true) => {
    ++creationDialogSession.current
    creationDialogRef.current = null
    setCreationDialog(null)
    // Restore after the closing render removes inert from the background.
    if (!restoreFocus) creationDialogOpener.current = null
  }, [])

  const openCreationDialog = (kind: 'board' | 'thread', opener: HTMLButtonElement) => {
    if (postHistory || deletedOpen || managementTarget || (kind === 'board' ? !canCreateBoard : !canCreateThread)) return
    creationDialogOpener.current = opener
    ++creationDialogSession.current
    creationDialogRef.current = kind
    setCreationDialog(kind)
  }

  const recheckCreation = useCallback(async (): Promise<CreationStatus | null> => {
    setCreationCheck('loading')
    setCreationError('')
    try {
      const status = await guiCreationStatus()
      creationStatusRef.current = status
      setCreationStatus(status)
      setCreationCheck('ready')
      return status
    } catch {
      setCreationCheck('failed')
      setCreationError(t.creationUnavailable)
      return null
    }
  }, [t.creationUnavailable])

  const clearPrivateWorkspace = useCallback(() => {
    ++searchNavigationRequest.current
    setSearchOpen(false)
    setSearchHighlight(new Set())
    setSearchQuery('')
    ++treeRequest.current
    ++capabilityRequest.current
    ++detailRequest.current
    ++pageRequest.current
    ++relationRequest.current
    pendingConversationJump.current = null
    setCollapsedConversationPosts(new Set())
    ++deletedRequest.current
    ++pinsRequest.current
    ++pinMutationRequest.current
    pinMutationRunning.current = false
    ++activityRequest.current
    closePostEditor()
    closePostHistory()
    setDeletedItems(null)
    setDeletedOpen(false)
    setDeletedSelectionMode(false)
    setSelectedDeletedKeys(new Set())
    setBulkManagementAction(null)
    setManagementTarget(null)
    setManagementError('')
    closeCreationDialog(false)
    setCapabilities(null)
    setCapabilityState('failed')
    setBoards([])
    setThreads([])
    setPinnedThreads([])
    setPinnedState('failed')
    setPinBusyId(null)
    setPinError('')
    setActivityItems([])
    setActivityState('ready')
    setActivityLoadingMore(false)
    activityLoadingMoreRef.current = false
    setActivityError(false)
    setActivityNextId(null)
    setTreeState('failed')
    selectedIdRef.current = null
    setSelectedId(null)
    setCurrentThread(null)
    setPosts([])
    setThreadPage({ page: 1, pageSize: 50, totalPosts: 0, totalPages: 1 })
    scrollCommit.current = null
    suppressThreadScrollSave.current = false
    setThreadScrollStatus(null)
    setPaging(false)
    setPagingError(false)
    setResponses(null)
    setResponseNext(null)
    setResponsePaging(false)
    setResponseState(null)
    setParentPost(null)
    setReplyTarget(null)
    setDetailState('ready')
  }, [closeCreationDialog, closePostEditor, closePostHistory])

  usePrivateViewGuard(accountId, selectedId, clearPrivateWorkspace, onSessionExpired, capabilities)
  // Our own restore/purge removes rows too. Its authenticated operation and refresh
  // own this interval; an older deleted-list snapshot must not revoke the result.
  useSettingsCapabilityGuard(accountId, 'conversation.deleted', deletedOpen && !managementBusy ? deletedItems : null, clearPrivateWorkspace)

  const refreshPinnedThreads = useCallback(async () => {
    const request = ++pinsRequest.current
    setPinnedState('loading')
    try {
      const result = await guiRead('conversation.pins')
      if (request !== pinsRequest.current) return
      setPinnedThreads(result.items)
      setPinnedState('ready')
      return result.items
    } catch (error) {
      if (request !== pinsRequest.current) return
      if (isAccessFault(error)) { clearPrivateWorkspace(); return }
      setPinnedState('failed')
      return null
    }
  }, [clearPrivateWorkspace])

  useEffect(() => {
    const accountChanged = accountIdRef.current !== accountId
    accountIdRef.current = accountId
    if (accountChanged) {
      ++pinsRequest.current
      ++pinMutationRequest.current
      pinMutationRunning.current = false
      setPinnedThreads([])
      setPinnedState('loading')
      setPinBusyId(null)
      setPinError('')
      ++activityRequest.current
      activityLoadingMoreRef.current = false
      setActivityItems([])
      setActivityState('ready')
      setActivityLoadingMore(false)
      setActivityError(false)
      setActivityNextId(null)
    }
    void refreshPinnedThreads()
  }, [accountId, refreshPinnedThreads])

  const refreshActivity = useCallback(async () => {
    const request = ++activityRequest.current
    activityLoadingMoreRef.current = false
    setActivityLoadingMore(false)
    setActivityState('loading')
    setActivityError(false)
    try {
      const result = await guiRead('conversation.activity', { limit: 50 })
      if (request !== activityRequest.current) return
      setActivityItems(result.items)
      setActivityNextId(result.next_before_id)
      setActivityState('ready')
    } catch (error) {
      if (request !== activityRequest.current) return
      if (isAccessFault(error)) { clearPrivateWorkspace(); return }
      setActivityState('failed')
    }
  }, [clearPrivateWorkspace])

  const loadMoreActivity = async () => {
    const beforeId = activityNextId
    if (beforeId === null || activityLoadingMoreRef.current || activityState !== 'ready') return
    const request = activityRequest.current
    activityLoadingMoreRef.current = true
    setActivityLoadingMore(true)
    setActivityError(false)
    try {
      const result = await guiRead('conversation.activity', { limit: 50, before_id: beforeId })
      if (request !== activityRequest.current) return
      setActivityItems((current) => {
        const seen = new Set(current.map((item) => item.id))
        return [...current, ...result.items.filter((item) => !seen.has(item.id))]
      })
      setActivityNextId(result.next_before_id !== null && result.next_before_id < beforeId ? result.next_before_id : null)
    } catch (error) {
      if (request !== activityRequest.current) return
      if (isAccessFault(error)) { clearPrivateWorkspace(); return }
      setActivityError(true)
    } finally {
      if (request === activityRequest.current) {
        activityLoadingMoreRef.current = false
        setActivityLoadingMore(false)
      }
    }
  }

  const notifyActivityMutation = () => {
    if (activeWorkspaceTabRef.current === 'activity') void refreshActivity()
  }

  useEffect(() => {
    if (activeWorkspaceTab === 'activity') void refreshActivity()
  }, [accountId, activeWorkspaceTab, refreshActivity])

  const toggleThreadPin = async () => {
    const thread = currentThread
    if (!thread || pinMutationRunning.current || pinnedState !== 'ready') return
    const isPinned = pinnedThreads.some((item) => item.id === thread.id)
    const request = ++pinMutationRequest.current
    pinMutationRunning.current = true
    setPinBusyId(thread.id)
    setPinError('')
    try {
      const result = await guiPinThread(thread.id, !isPinned)
      if (request !== pinMutationRequest.current) return
      setPinnedThreads((current) => result.pinned
        ? current.some((item) => item.id === thread.id) ? current : [...current, thread]
        : current.filter((item) => item.id !== result.id))
      setPinError('')
      notifyActivityMutation()
    } catch (error) {
      if (request !== pinMutationRequest.current) return
      if (isAccessFault(error)) { clearPrivateWorkspace(); return }
      const failure = pinSaveFailedLabel(language)
      setPinError(language === 'ja' ? `${failure}（${thread.name}）` : `${failure} · ${thread.name}`)
    } finally {
      if (request === pinMutationRequest.current) {
        pinMutationRunning.current = false
        setPinBusyId(null)
      }
    }
  }

  const loadTree = useCallback(async (keepSelection = true): Promise<Thread[] | null> => {
    closePostEditor()
    closePostHistory()
    const request = ++treeRequest.current
    const permissionRequest = ++capabilityRequest.current
    setTreeState('loading')
    setCapabilities(null)
    setCapabilityState('loading')
    void guiRead('conversation.capabilities').then((result) => {
      if (request !== treeRequest.current || permissionRequest !== capabilityRequest.current) return
      setCapabilities(result)
      setCapabilityState('ready')
      const selected = selectedIdRef.current
      if (selected && !result.threads.some((item) => item.id === selected && item.response_create)) setReplyTarget(null)
    }).catch((error: unknown) => {
      if (request !== treeRequest.current || permissionRequest !== capabilityRequest.current) return
      if (isAccessFault(error)) { clearPrivateWorkspace(); return }
      setCapabilities(null)
      setCapabilityState('failed')
      setReplyTarget(null)
    })
    try {
      const [boardResult, threadResult] = await Promise.all([
        readAllPages((args) => guiRead('board.list', args), () => request === treeRequest.current),
        readAllPages((args) => guiRead('thread.list', args), () => request === treeRequest.current),
      ])
      if (request !== treeRequest.current) return null
      setBoards(boardResult.items)
      setThreads(threadResult.items)
      setTreeState('ready')
      if (!keepSelection || (selectedId && !threadResult.items.some((thread) => thread.id === selectedId))) {
        ++pageRequest.current
        selectedIdRef.current = null
        setSelectedId(null)
        setCurrentThread(null)
        setPosts([])
        setThreadPage({ page: 1, pageSize: 50, totalPosts: 0, totalPages: 1 })
        setResponses(null)
        setResponseNext(null)
        setResponseState(null)
        setParentPost(null)
        setReplyTarget(null)
        setDetailState('ready')
      }
      return threadResult.items
    } catch {
      if (request !== treeRequest.current) return null
      ++detailRequest.current
      ++pageRequest.current
      ++relationRequest.current
      setBoards([])
      setThreads([])
      ++capabilityRequest.current
      setCapabilities(null)
      setCapabilityState('failed')
      selectedIdRef.current = null
      setSelectedId(null)
      setCurrentThread(null)
      setPosts([])
      setThreadPage({ page: 1, pageSize: 50, totalPosts: 0, totalPages: 1 })
      setPaging(false)
      setPagingError(false)
      setResponses(null)
      setResponseNext(null)
      setResponsePaging(false)
      setParentPost(null)
      setResponseState(null)
      setReplyTarget(null)
      setTreeState('failed')
      setDetailState('ready')
      return null
    }
  }, [clearPrivateWorkspace, closePostEditor, closePostHistory, selectedId])

  const loadDeletedItems = useCallback(async (allowOpening = false) => {
    const request = ++deletedRequest.current
    const tree = treeRequest.current
    const permission = capabilityRequest.current
    setDeletedState('loading')
    try {
      const result = await guiRead('conversation.deleted')
      if (request !== deletedRequest.current || tree !== treeRequest.current || permission !== capabilityRequest.current || (!deletedOpen && !allowOpening)) return
      setDeletedItems(result)
      setDeletedState('ready')
    } catch (error) {
      if (request !== deletedRequest.current || tree !== treeRequest.current || permission !== capabilityRequest.current || (!deletedOpen && !allowOpening)) return
      if (isAccessFault(error)) { clearPrivateWorkspace(); return }
      setDeletedItems(null)
      setDeletedState('failed')
    }
  }, [clearPrivateWorkspace, deletedOpen])

  useEffect(() => { void loadTree(false) }, [])
  useEffect(() => { void recheckCreation() }, [recheckCreation])
  useEffect(() => {
    if (capabilityState !== 'ready') return
    const allowed = capabilities?.boards.filter((item) => item.thread_create && boards.some((board) => board.id === item.id)).map((item) => item.id) || []
    setThreadBoardId((current) => current && allowed.includes(current) ? current : allowed[0] || '')
  }, [boards, capabilities, capabilityState])

  const selectThread = useCallback(async (thread: Thread, anchorPostNo?: number, searchNavigation = false, verifiedThread?: Thread) => {
    ++searchNavigationRequest.current
    setSearchNavigationError('')
    const previousThreadId = currentThreadRef.current?.id
    if (previousThreadId) persistThreadScroll(previousThreadId)
    closePostEditor()
    closePostHistory()
    pendingConversationJump.current = null
    setCollapsedConversationPosts(new Set())
    setManagementTarget(null)
    setPinError('')
    const request = ++detailRequest.current
    const pageGeneration = ++pageRequest.current
    const permissionRequest = ++capabilityRequest.current
    ++relationRequest.current
    scrollCommit.current = null
    suppressThreadScrollSave.current = true
    setThreadScrollStatus('restoring')
    let scrollPreference = { ...DEFAULT_THREAD_SCROLL_PREFERENCE }
    try {
      const savedPreference = await loadThreadScrollPreference(thread.id, request)
      if (request !== detailRequest.current || accountIdRef.current !== accountId) return
      scrollPreference = anchorPostNo === undefined
        ? savedPreference
        : { ...savedPreference, postNo: anchorPostNo, offset: 0, scrollTop: 0, atBottom: false }
    } catch {
      if (request !== detailRequest.current || accountIdRef.current !== accountId) return
      setThreadScrollStorageError('read')
    }
    selectedIdRef.current = thread.id
    setCapabilities(null)
    setCapabilityState('loading')
    void guiRead('conversation.capabilities').then((result) => {
      if (request !== detailRequest.current || permissionRequest !== capabilityRequest.current) return
      setCapabilities(result)
      setCapabilityState('ready')
      if (!result.threads.some((item) => item.id === thread.id && item.response_create)) setReplyTarget(null)
    }).catch((error: unknown) => {
      if (request !== detailRequest.current || permissionRequest !== capabilityRequest.current) return
      if (isAccessFault(error)) { clearPrivateWorkspace(); return }
      setCapabilities(null)
      setCapabilityState('failed')
      setReplyTarget(null)
    })
    setSelectedId(thread.id)
    setCurrentThread(null)
    setPosts([])
    setThreadPage({ page: 1, pageSize: scrollPreference.pageSize || 50, totalPosts: 0, totalPages: 1 })
    setResponses(null)
    setResponseNext(null)
    setResponsePaging(false)
    setResponseState(null)
    setParentPost(null)
    setPaging(false)
    setPagingError(false)
    setDetailState('loading')
    try {
      let missingAnchor = false
      const readSavedPage = async () => {
        try {
          return await guiRead('post.page', {
            thread_id: thread.id,
            page: scrollPreference.page || 1,
            page_size: scrollPreference.pageSize || 50,
            ...(scrollPreference.postNo !== null ? { anchor_no: scrollPreference.postNo } : {}),
          })
        } catch (error) {
          if (anchorPostNo !== undefined || scrollPreference.postNo === null || typeof error !== 'object' || error === null || !('code' in error) || error.code !== 'NOT_FOUND') throw error
          if (request !== detailRequest.current || pageGeneration !== pageRequest.current) throw error
          // A removed anchor cannot revoke access to an otherwise readable Thread.
          // The retry still passes through the same current permission checks.
          missingAnchor = true
          return guiRead('post.page', { thread_id: thread.id, page: scrollPreference.page || 1, page_size: scrollPreference.pageSize || 50 })
        }
      }
      const [item, latestPage] = await Promise.all([
        verifiedThread ? Promise.resolve(verifiedThread) : guiRead('thread.get', { id: thread.id }),
        readSavedPage(),
      ])
      if (request !== detailRequest.current || pageGeneration !== pageRequest.current) return
      if (item.id !== thread.id) throw new Error('The selected Thread changed while opening the search result.')
      let page = latestPage
      let restoreFailed = missingAnchor
      if (anchorPostNo !== undefined) {
        const anchoredPost = latestPage.items.find((post) => post.no === anchorPostNo)
        if (!anchoredPost || anchoredPost.deleted || anchoredPost.body === null) throw new Error('Search result is no longer available in its requested page.')
      }
      if (!missingAnchor && scrollPreference.postNo !== null && !latestPage.items.some((post) => post.no === scrollPreference.postNo)) {
        try {
          page = await guiRead('post.page', { thread_id: thread.id, page: scrollPreference.page || 1, page_size: scrollPreference.pageSize || 50 })
          if (request !== detailRequest.current || pageGeneration !== pageRequest.current) return
          restoreFailed = true
        } catch (error) {
          if (request !== detailRequest.current || pageGeneration !== pageRequest.current) return
          if (isAccessFault(error)) { clearPrivateWorkspace(); return }
          throw error
        }
      }
      const restoredPreference: ThreadScrollPreference = restoreFailed
        ? { ...DEFAULT_THREAD_SCROLL_PREFERENCE, page: page.page, pageSize: page.page_size as ThreadPageSize }
        : { ...scrollPreference, page: page.page, pageSize: page.page_size as ThreadPageSize }
      scrollCommit.current = {
        threadId: thread.id,
        generation: request,
        pageGeneration,
        preference: restoredPreference,
        mode: 'restore',
        failed: restoreFailed,
      }
      setCurrentThread(item)
      setPosts(page.items)
      if (anchorPostNo !== undefined) {
        const ancestors = getConversationAncestors(buildConversationTree(page.items), anchorPostNo)
        setCollapsedConversationPosts((current) => {
          const next = new Set(current)
          ancestors.forEach((ancestor) => next.delete(ancestor))
          return next
        })
      }
      setThreadPage({ page: page.page, pageSize: page.page_size as ThreadPageSize, totalPosts: page.total_posts, totalPages: page.total_pages })
      setDetailState('ready')
      if (searchNavigation) {
        const highlightKey = anchorPostNo === undefined
          ? `thread:${item.id}`
          : `post:${page.items.find((post) => post.no === anchorPostNo)!.id}`
        setSearchHighlight((current) => new Set(current).add(highlightKey))
        if (item.board_id) setCollapsedBoards((current) => {
          const next = new Set(current)
          next.delete(item.board_id!)
          return next
        })
        requestAnimationFrame(() => {
          if (request !== detailRequest.current) return
          selectedThreadButton.current?.scrollIntoView?.({ block: 'nearest' })
          if (anchorPostNo === undefined) selectedThreadButton.current?.focus({ preventScroll: true })
        })
      }
      return true
    } catch (error) {
      if (request !== detailRequest.current || pageGeneration !== pageRequest.current) return
      scrollCommit.current = null
      suppressThreadScrollSave.current = false
      setThreadScrollStatus(null)
      if (isAccessFault(error)) { clearPrivateWorkspace(); return }
      if (searchNavigation || anchorPostNo !== undefined) setSearchNavigationError(words[language].searchNavigationFailed)
      setCurrentThread(null)
      setPosts([])
      setThreadPage({ page: 1, pageSize: scrollPreference.pageSize || 50, totalPosts: 0, totalPages: 1 })
      setDetailState('failed')
    }
  }, [accountId, clearPrivateWorkspace, closePostEditor, closePostHistory, language])

  useEffect(() => {
    let active = true
    let timer: ReturnType<typeof setTimeout> | undefined
    let handled: string | null = null
    const check = async () => {
      try {
        if (treeState !== 'ready') return
        const activation = await guiNotificationActivation()
        if (!active || activation.state === 'none') return
        if (activation.state !== 'ready') {
          setNotificationError(language === 'ja'
            ? activation.state === 'unavailable' ? '通知先のThreadは現在閲覧できません。' : '通知を受け取ったアカウントでログインしてください。'
            : activation.state === 'unavailable' ? 'The notification Thread is no longer available.' : 'Sign in to the account that received this notification.')
          if (activation.state === 'unavailable') await guiNotificationAck(activation.event_id)
          return
        }
        if (activation.account_id !== accountId || activation.event_id === handled) return
        handled = activation.event_id
        setNotificationError('')
        const opened = await selectThread(activation.thread)
        if (!active) return
        if (opened) {
          await guiNotificationAck(activation.event_id)
        } else {
          setNotificationError(language === 'ja' ? '通知先のThreadを開けませんでした。' : 'Could not open the notification Thread.')
        }
      } catch {
        if (active) setNotificationError(language === 'ja' ? '通知の確認に失敗しました。' : 'Could not check the notification.')
      } finally {
        if (active) timer = setTimeout(() => void check(), 1000)
      }
    }
    void check()
    return () => { active = false; if (timer) clearTimeout(timer) }
  }, [accountId, language, notificationRetryToken, refreshToken, selectThread, treeState])

  const navigateSearchResult = useCallback(async (item: SearchItem): Promise<void> => {
    const request = ++searchNavigationRequest.current
    setSearchNavigationError('')
    if (item.kind === 'board') {
      try {
        const currentBoards = await readAllPages((args) => guiRead('board.list', args), () => request === searchNavigationRequest.current)
        if (request !== searchNavigationRequest.current) return
        const board = currentBoards.items.find((candidate) => candidate.id === item.id)
        if (!board) {
          setSearchNavigationError(words[language].searchNavigationFailed)
          return
        }
        setBoards(currentBoards.items)
        setSearchHighlight((current) => new Set(current).add(`board:${board.id}`))
        setActiveWorkspaceTab('boards')
        setCollapsedBoards((current) => {
          if (!current.has(board.id)) return current
          const next = new Set(current)
          next.delete(board.id)
          return next
        })
        requestAnimationFrame(() => {
          if (request !== searchNavigationRequest.current) return
          const target = boardToggleRefs.current[board.id]
          target?.scrollIntoView?.({ block: 'nearest' })
          target?.focus({ preventScroll: true })
        })
      } catch (error) {
        if (request !== searchNavigationRequest.current) return
        if (isAccessFault(error)) { clearPrivateWorkspace(); return }
        setSearchNavigationError(words[language].searchNavigationFailed)
      }
      return
    }

    const threadId = item.kind === 'thread' ? item.id : item.thread_id
    if (!threadId) {
      setSearchNavigationError(words[language].searchNavigationFailed)
      return
    }
    try {
      const thread = await guiRead('thread.get', { id: threadId })
      if (request !== searchNavigationRequest.current) return
      if (item.kind === 'thread') {
        setActiveWorkspaceTab(thread.archived ? 'archive' : 'boards')
        await selectThread(thread, undefined, true, thread)
        return
      }
      if (item.post_no === null || !Number.isSafeInteger(item.post_no) || item.post_no < 1) throw new Error('Search result does not contain a valid Post number.')
      const post = await guiRead('post.get', { thread_id: thread.id, no: item.post_no })
      if (request !== searchNavigationRequest.current) return
      if (post.deleted || post.body === null || post.id !== item.id) throw new Error('Search result is no longer available.')
      setActiveWorkspaceTab(thread.archived ? 'archive' : 'boards')
      await selectThread(thread, item.post_no, true, thread)
    } catch (error) {
      if (request !== searchNavigationRequest.current) return
      if (isAccessFault(error)) { clearPrivateWorkspace(); return }
      setSearchNavigationError(words[language].searchNavigationFailed)
    }
  }, [clearPrivateWorkspace, language, selectThread])

  useLayoutEffect(() => {
    if (authorizationRefreshTokenRef.current === refreshToken) return
    authorizationRefreshTokenRef.current = refreshToken
    setSearchHighlight(new Set())
    const retainedId = selectedIdRef.current
    ++externalLinkRequest.current
    externalLinkBusyRef.current = false
    externalLinkOpener.current = null
    setPendingExternalUrl(null)
    setExternalLinkBusy(false)
    setExternalLinkError(false)
    ++searchNavigationRequest.current
    setSearchOpen(false)
    setSearchNavigationError('')
    if (retainedId) {
      persistThreadScroll(retainedId, false)
      ++detailRequest.current
      ++pageRequest.current
      ++relationRequest.current
      ++editRequest.current
      ++historyRequest.current
      scrollCommit.current = null
      suppressThreadScrollSave.current = false
      closePostEditor()
      closePostHistory()
      setCurrentThread(null)
      setPosts([])
      setThreadPage({ page: 1, pageSize: 50, totalPosts: 0, totalPages: 1 })
      setDetailState('loading')
      setPaging(false)
      setPagingError(false)
      setResponses(null)
      setResponseNext(null)
      setResponsePaging(false)
      setResponseState(null)
      setParentPost(null)
      setReplyTarget(null)
    }
    setCapabilities(null)
    setCapabilityState('loading')
    setManagementTarget(null)
    setManagementError('')
    setManagementNeedsRefresh(false)

    let cancelled = false
    void (async () => {
      const [freshThreads] = await Promise.all([
        loadTree(true),
        recheckCreation(),
        refreshPinnedThreads(),
        refreshActivity(),
      ])
      if (cancelled || !retainedId || selectedIdRef.current !== retainedId || !freshThreads) return
      const retainedThread = freshThreads.find((thread) => thread.id === retainedId)
      if (retainedThread) await selectThread(retainedThread)
    })()
    return () => { cancelled = true }
  }, [closePostEditor, closePostHistory, loadTree, recheckCreation, refreshActivity, refreshPinnedThreads, refreshToken, selectThread])

  async function requestThreadPage(targetPage: number, targetPageSize = threadPage.pageSize, preserveAnchor = false) {
    if (!currentThread || paging || postEditRunning.current || postEditBusy) return
    const requestedThread = currentThread.id
    const request = detailRequest.current
    const generation = ++pageRequest.current
    ++relationRequest.current
    closePostEditor()
    closePostHistory()
    setManagementTarget(null)
    setResponses(null)
    setResponseNext(null)
    setResponsePaging(false)
    setResponseState(null)
    setParentPost(null)
    const previous = preserveAnchor ? readCurrentThreadScroll() : null
    const anchorNo = previous?.postNo ?? null
    setPaging(true)
    setPagingError(false)
    try {
      const page = await guiRead('post.page', {
        thread_id: requestedThread,
        page: targetPage,
        page_size: targetPageSize,
        ...(anchorNo !== null ? { anchor_no: anchorNo } : {}),
      })
      if (request !== detailRequest.current || generation !== pageRequest.current || requestedThread !== selectedIdRef.current) return
      const preference: ThreadScrollPreference = preserveAnchor && previous
        ? { ...previous, postNo: anchorNo, atBottom: false, page: page.page, pageSize: page.page_size as ThreadPageSize }
        : { ...DEFAULT_THREAD_SCROLL_PREFERENCE, page: page.page, pageSize: page.page_size as ThreadPageSize }
      saveThreadScrollPreference(requestedThread, preference)
      setCollapsedConversationPosts(new Set())
      scrollCommit.current = { threadId: requestedThread, generation: request, pageGeneration: generation, preference, mode: 'restore' }
      suppressThreadScrollSave.current = true
      setPosts(page.items)
      setThreadPage({ page: page.page, pageSize: page.page_size as ThreadPageSize, totalPosts: page.total_posts, totalPages: page.total_pages })
    } catch (error) {
      if (request === detailRequest.current && generation === pageRequest.current) {
        scrollCommit.current = null
        suppressThreadScrollSave.current = false
        persistThreadScroll(requestedThread, false)
        if (isAccessFault(error)) clearPrivateWorkspace()
        else setPagingError(true)
      }
    } finally {
      if (request === detailRequest.current && generation === pageRequest.current) setPaging(false)
    }
  }

  function changePageSize(value: string) {
    const parsed = Number(value)
    if (parsed !== 50 && parsed !== 100 && parsed !== 500 && parsed !== 1000) return
    void requestThreadPage(threadPage.page, parsed, true)
  }

  function preserveConversationAnchor(anchorOverride?: ThreadScrollPreference, preservePreference = false) {
    if (!currentThread) return
    const current = anchorOverride || readCurrentThreadScroll()
    if (!current) return
    const preference: ThreadScrollPreference = { ...current, page: threadPage.page, pageSize: threadPage.pageSize }
    scrollCommit.current = { threadId: currentThread.id, generation: detailRequest.current, pageGeneration: pageRequest.current, preference, mode: 'restore', preservePreference }
    suppressThreadScrollSave.current = true
    saveThreadScrollPreference(currentThread.id, preference)
  }

  function changeConversationView(view: ConversationView) {
    if (view === conversationView) return
    viewEditSequenceRef.current += 1
    const preference = readCurrentThreadScroll()
    if (view === 'threaded' && preference?.postNo !== null && preference?.postNo !== undefined) {
      const ancestors = getConversationAncestors(conversationRows, preference.postNo)
      if (ancestors.length) {
        setCollapsedConversationPosts((current) => {
          const next = new Set(current)
          ancestors.forEach((ancestor) => next.delete(ancestor))
          return next
        })
      }
    }
    preserveConversationAnchor(preference || undefined)
    setConversationView(view)
  }

  function toggleConversationPost(no: number) {
    const collapsing = !collapsedConversationPosts.has(no)
    let preference = readCurrentThreadScroll()
    let preservePreference = false
    if (collapsing && preference?.postNo !== null && preference?.postNo !== undefined && getConversationAncestors(conversationRows, preference.postNo).includes(no)) {
      preference = { ...preference, postNo: no, offset: 0, atBottom: false }
      preservePreference = true
    }
    preserveConversationAnchor(preference || undefined, preservePreference)
    setCollapsedConversationPosts((current) => {
      const next = new Set(current)
      if (next.has(no)) next.delete(no)
      else next.add(no)
      return next
    })
  }

  async function openResponses(post: Post) {
    if (!post.response_count || !currentThread) return
    const request = ++relationRequest.current
    setResponses(null)
    setResponseNext(null)
    setResponsePaging(false)
    setParentPost(null)
    setResponseState('loading')
    try {
      const page = await guiRead('response.list', { thread_id: currentThread.id, to: post.no, limit: 50 })
      if (request !== relationRequest.current) return
      setResponses({ target: post.no, items: page.items.filter((item) => !item.deleted) })
      setResponseNext(page.next_before_no)
      setResponseState('ready')
    } catch (error) {
      if (request !== relationRequest.current) return
      if (isAccessFault(error)) { clearPrivateWorkspace(); return }
      setResponseState('failed')
    }
  }

  async function followReply(no: number) {
    if (!currentThread) return
    const visible = posts.find((post) => post.no === no)
    if (visible) {
      if (conversationView === 'threaded') {
        const ancestors = getConversationAncestors(conversationRows, no)
        const collapsedAncestor = ancestors.some((ancestor) => collapsedConversationPosts.has(ancestor))
        if (collapsedAncestor) {
          pendingConversationJump.current = { threadId: currentThread.id, postNo: no, detailGeneration: detailRequest.current, pageGeneration: pageRequest.current }
          setCollapsedConversationPosts((current) => {
            const next = new Set(current)
            ancestors.forEach((ancestor) => next.delete(ancestor))
            return next
          })
          return
        }
      }
      threadScrollElement.current?.querySelector<HTMLElement>(`#post-${no}`)?.scrollIntoView?.({ behavior: 'smooth', block: 'center' })
      return
    }
    const request = ++relationRequest.current
    setResponses(null)
    setResponseNext(null)
    setResponsePaging(false)
    setParentPost(null)
    setResponseState('loading')
    let parent: Post | null = null
    let pageGeneration: number | null = null
    try {
      const threadId = currentThread.id
      const detailGeneration = detailRequest.current
      parent = await guiRead('post.get', { thread_id: threadId, no })
      if (request !== relationRequest.current) return
      if (parent.thread_id !== threadId || parent.no !== no) throw new Error('The reply target changed while it was being opened.')
      pageGeneration = ++pageRequest.current
      setPaging(true)
      setPagingError(false)
      const page = await guiRead('post.page', { thread_id: threadId, anchor_no: no, page_size: threadPage.pageSize })
      if (request !== relationRequest.current || detailGeneration !== detailRequest.current || pageGeneration !== pageRequest.current || selectedIdRef.current !== threadId) return
      if (!page.items.some((item) => item.no === no && item.thread_id === threadId)) {
        setParentPost(parent)
        setResponseState('ready')
        setPagingError(true)
        return
      }
      closePostEditor()
      closePostHistory()
      setManagementTarget(null)
      setResponses(null)
      setResponseNext(null)
      setResponsePaging(false)
      setResponseState(null)
      setParentPost(null)
      setCollapsedConversationPosts(new Set())
      const preference: ThreadScrollPreference = { ...DEFAULT_THREAD_SCROLL_PREFERENCE, postNo: no, page: page.page, pageSize: page.page_size as ThreadPageSize }
      scrollCommit.current = { threadId, generation: detailGeneration, pageGeneration, preference, mode: 'restore' }
      suppressThreadScrollSave.current = true
      setThreadScrollStatus('restoring')
      saveThreadScrollPreference(threadId, preference)
      setPosts(page.items)
      setThreadPage({ page: page.page, pageSize: page.page_size as ThreadPageSize, totalPosts: page.total_posts, totalPages: page.total_pages })
    } catch (error) {
      if (request !== relationRequest.current) return
      if (isAccessFault(error)) { clearPrivateWorkspace(); return }
      setParentPost(parent)
      setResponseState(parent ? 'ready' : 'failed')
      if (pageGeneration !== null) setPagingError(true)
    } finally {
      if (pageGeneration !== null && request === relationRequest.current && pageGeneration === pageRequest.current) setPaging(false)
    }
  }

  async function loadOlderResponses() {
    if (!responses || responseNext === null || responsePaging || !currentThread) return
    const request = relationRequest.current
    const target = responses.target
    setResponsePaging(true)
    try {
      const page = await guiRead('response.list', { thread_id: currentThread.id, to: target, limit: 50, before_no: responseNext })
      if (request !== relationRequest.current || target !== responses.target) return
      setResponses((current) => {
        if (!current || current.target !== target) return current
        const seen = new Set(current.items.map((post) => post.no))
        return { ...current, items: [...page.items.filter((post) => !post.deleted && !seen.has(post.no)), ...current.items].sort((a, b) => a.no - b.no) }
      })
      setResponseNext(page.next_before_no)
    } catch (error) {
      if (request === relationRequest.current) {
        if (isAccessFault(error)) clearPrivateWorkspace()
        else setResponseState('failed')
      }
    } finally { if (request === relationRequest.current) setResponsePaging(false) }
  }

  function editPost(post: Post) {
    if (post.deleted || post.body === null || !currentThread || selectedCapabilities?.post_edit !== true || creationStatus.pending || creationBusy || busy || postEditRunning.current) return
    closePostHistory()
    closePostEditor()
    setPostEditor({ threadId: currentThread.id, no: post.no, baseRevision: post.revision, draft: post.body, latest: null, saving: false, reloading: false, conflict: false, reviewedLatest: false, error: '' })
  }

  async function reloadLatestPost() {
    const editing = postEditor
    if (!editing || !editing.conflict || editing.saving || editing.reloading || !currentThread || selectedIdRef.current !== editing.threadId) return
    const request = editRequest.current
    setPostEditor((current) => current?.threadId === editing.threadId && current.no === editing.no ? { ...current, reloading: true, error: '' } : current)
    try {
      const latest = await guiRead('post.get', { thread_id: editing.threadId, no: editing.no })
      if (request !== editRequest.current || selectedIdRef.current !== editing.threadId) return
      if (latest.deleted || latest.body === null) {
        closePostEditor()
        await refresh()
        return
      }
      setPostEditor((current) => current?.threadId === editing.threadId && current.no === editing.no
        ? { ...current, baseRevision: latest.revision, latest, reloading: false, conflict: false, reviewedLatest: true, error: '' }
        : current)
    } catch (error) {
      if (request !== editRequest.current || selectedIdRef.current !== editing.threadId) return
      if (isAccessFault(error)) { clearPrivateWorkspace(); return }
      setPostEditor((current) => current?.threadId === editing.threadId && current.no === editing.no
        ? { ...current, reloading: false, error: t.editFailed }
        : current)
    }
  }

  async function submitPostEdit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    const editing = postEditor
    if (!editing || postEditRunning.current || editing.saving || editing.reloading || creationStatus.pending || creationBusy || busy || !currentThread || currentThread.id !== editing.threadId || selectedCapabilities?.post_edit !== true) return
    if (editing.draft.trim().length === 0) {
      setPostEditor({ ...editing, error: t.bodyRequired })
      return
    }
    if (new TextEncoder().encode(editing.draft).byteLength > 1024 * 1024) {
      setPostEditor({ ...editing, error: t.bodyTooLarge })
      return
    }
    if (characterCount(editing.draft) > BODY_LIMIT) {
      setPostEditor({ ...editing, error: t.bodyTooLong })
      return
    }
    if (!Number.isSafeInteger(editing.no) || editing.no < 1 || !Number.isSafeInteger(editing.baseRevision) || editing.baseRevision < 1 || !editing.threadId) {
      setPostEditor({ ...editing, error: t.revisionInvalid })
      return
    }
    if (editing.conflict && !editing.reviewedLatest) return
    postEditRunning.current = true
    setPostEditBusy(true)
    const request = ++editRequest.current
    setPostEditor({ ...editing, saving: true, error: '' })
    try {
      const saved = await guiEdit({ thread_id: editing.threadId, no: editing.no, body: editing.draft, if_revision: editing.baseRevision })
      if (request !== editRequest.current || selectedIdRef.current !== editing.threadId) return
      if (saved.thread_id !== editing.threadId || saved.no !== editing.no || saved.body !== editing.draft) {
        setPostEditor({ ...editing, saving: false, error: t.editFailed })
        return
      }
      closePostEditor()
      setCreationNotice(t.editSaved)
      notifyActivityMutation()
      await selectThread(currentThread)
    } catch (error) {
      if (request !== editRequest.current || selectedIdRef.current !== editing.threadId) return
      if (isAccessFault(error)) { clearPrivateWorkspace(); return }
      const code = faultExitCode(error)
      if (code === 6) setPostEditor({ ...editing, saving: false, conflict: true, reviewedLatest: false, error: t.editConflict })
      else if (code === 8) setPostEditor({ ...editing, saving: false, conflict: true, reviewedLatest: false, error: t.editCommunication })
      else if (code === 2) setPostEditor({ ...editing, saving: false, conflict: false, reviewedLatest: false, error: t.editFailed })
      else setPostEditor({ ...editing, saving: false, conflict: true, reviewedLatest: false, error: t.editCommunication })
    } finally {
      postEditRunning.current = false
      setPostEditBusy(false)
    }
  }

  async function openPostHistory(post: Post, opener: HTMLButtonElement) {
    if (post.deleted || post.body === null || !currentThread || selectedCapabilities?.post_history !== true) return
    closePostEditor()
    closePostHistory()
    const request = ++historyRequest.current
    const threadId = currentThread.id
    historyOpener.current = opener
    setPostHistory({ threadId, no: post.no, state: 'loading', current: null, items: [] })
    try {
      const [current, result] = await Promise.all([
        guiRead('post.get', { thread_id: threadId, no: post.no }),
        guiRead('post.history', { thread_id: threadId, no: post.no }),
      ])
      if (request !== historyRequest.current || selectedIdRef.current !== threadId) return
      if (current.deleted || current.body === null) { closePostHistory(); return }
      const live = postsRef.current.find((item) => item.no === post.no)
      if (!live || live.deleted || live.body === null) { closePostHistory(); return }
      setPostHistory({ threadId, no: post.no, state: 'ready', current, items: result.items })
    } catch (error) {
      if (request !== historyRequest.current || selectedIdRef.current !== threadId) return
      if (isAccessFault(error)) { clearPrivateWorkspace(); return }
      setPostHistory({ threadId, no: post.no, state: 'failed', current: null, items: [] })
    }
  }

  const refresh = async (includeAuxiliary = true) => {
    const retainedId = selectedId
    ++detailRequest.current
    const selectionRequest = detailRequest.current
    ++relationRequest.current
    setCurrentThread(null)
    setPosts([])
    setResponses(null)
    setResponseNext(null)
    setResponsePaging(false)
    setParentPost(null)
    setResponseState(null)
    if (retainedId) setDetailState('loading')
    const freshThreads = await loadTree(true)
    if (!freshThreads) return false
    if (includeAuxiliary) await refreshPinnedThreads()
    if (retainedId && freshThreads && selectionRequest === detailRequest.current) {
      const current = freshThreads.find((thread) => thread.id === retainedId)
      if (current && !await selectThread(current)) return false
    }
    if (includeAuxiliary && deletedOpen) await loadDeletedItems()
    if (includeAuxiliary && activeWorkspaceTabRef.current === 'activity') await refreshActivity()
    setManagementNeedsRefresh(false)
    return true
  }

  const refreshCurrentThread = async () => {
    const thread = currentThread
    if (!thread) return
    await runScopedRefresh(() => selectThread(thread))
  }

  const refreshAllThreads = async () => {
    if (treeState === 'ready' && threads.length === 0) return
    await runScopedRefresh(() => refresh(false))
  }

  const refreshPinnedScope = async () => {
    if (pinnedState === 'ready' && pinnedThreads.length === 0) return
    await runScopedRefresh(reloadPinnedScope)
  }

  const runScopedRefresh = async (run: () => Promise<boolean | undefined>) => {
    if (scopedRefreshRunning.current) return
    scopedRefreshRunning.current = true
    notifyRefreshFeedback('updating')
    try {
      const completed = await run()
      notifyRefreshFeedback(completed ? 'updated' : 'refreshFailed')
    } catch {
      notifyRefreshFeedback('refreshFailed')
    } finally {
      scopedRefreshRunning.current = false
    }
  }

  const reloadPinnedScope = async () => {
    const selection = selectedIdRef.current
    const generation = detailRequest.current
    const freshPins = await refreshPinnedThreads()
    if (!freshPins) return false
    // Only update cached entries belonging to the refreshed pin scope.
    setThreads((current) => current.map((thread) => freshPins.find((pin) => pin.id === thread.id) || thread))
    if (selection && generation === detailRequest.current && selection === selectedIdRef.current) {
      const selectedPin = freshPins.find((thread) => thread.id === selection)
      if (selectedPin) return await selectThread(selectedPin)
    }
    return true
  }

  const finishCreate = async (operation: CreateOperation, entity: CreatedEntity, receipt: DraftReceipt | null, dialogSession?: number, selectionRequest = detailRequest.current) => {
    const dialogKind = operation === 'board.create' ? 'board' : operation === 'thread.create' ? 'thread' : null
    if (dialogSession !== undefined && dialogKind && creationDialogSession.current === dialogSession && creationDialogRef.current === dialogKind) closeCreationDialog()
    if (receipt?.draftKey && receipt.body !== null) {
      setDrafts((current) => current[receipt.draftKey!] === receipt.body
        ? { ...current, [receipt.draftKey!]: '' }
        : current)
    }
    if (operation === 'board.create') {
      setBoardName('')
      setBoardDescription('')
    } else if (operation === 'thread.create') {
      setThreadName('')
      setThreadDescription('')
    } else if (operation === 'post.create' || operation === 'response.create') {
      setReplyTarget(null)
    }
    receiptRef.current = null
    setFormError('')
    setCreationNotice(t.created)
    setCreationError('')
    setCreationStatus(emptyCreationStatus)
    creationStatusRef.current = emptyCreationStatus
    setCreationCheck('ready')

    const freshThreads = await loadTree(true)
    if (operation === 'thread.create' && selectionRequest === detailRequest.current) {
      const created = entity as Thread
      const visible = freshThreads?.find((thread) => thread.id === created.id) || created
      await selectThread(visible)
    } else if (operation === 'post.create' || operation === 'response.create') {
      const destinationId = (entity as Post).thread_id
      if (destinationId && selectedIdRef.current === destinationId) {
        const visible = freshThreads?.find((thread) => thread.id === destinationId)
        if (visible) await selectThread(visible)
      }
    }
    await recheckCreation()
  }

  const acceptCreated = async (operation: CreateOperation, entity: CreatedEntity, receipt: DraftReceipt | null, dialogSession?: number, selectionRequest = detailRequest.current) => {
    notifyActivityMutation()
    try {
      await guiAckCreate()
    } catch {
      const status = await recheckCreation()
      if (status && !status.pending) {
        await finishCreate(operation, entity, receipt, dialogSession, selectionRequest)
        return
      }
      setCreationNotice(t.created)
      setCreationError(t.ackPending)
      return
    }
    await finishCreate(operation, entity, receipt, dialogSession, selectionRequest)
  }

  const runCreate = async (operation: CreateOperation, args: Record<string, unknown>, receipt: DraftReceipt | null = null, dialogSession?: number) => {
    if (creationRunning.current || creationCheck !== 'ready' || creationStatus.pending || busy) return
    const selectionRequest = detailRequest.current
    creationRunning.current = true
    setCreationBusy(true)
    setCreationError('')
    setCreationNotice('')
    receiptRef.current = receipt
    try {
      const entity = await guiCreate(operation, args)
      await acceptCreated(operation, entity, receipt, dialogSession, selectionRequest)
    } catch (error) {
      await recheckCreation()
      const threadId = typeof args.thread_id === 'string' ? args.thread_id : null
      if ((operation === 'post.create' || operation === 'response.create') && isThreadFullFault(error) && threadId && selectedIdRef.current === threadId && selectionRequest === detailRequest.current) {
        setThreadPage((current) => ({ ...current, totalPosts: Math.max(1000, current.totalPosts) }))
        setCreationError('')
      } else setCreationError(t.creationFailed)
    } finally {
      creationRunning.current = false
      setCreationBusy(false)
    }
  }

  const retryCreate = async () => {
    const status = creationStatusRef.current
    if (creationRunning.current || !status.pending || status.expired || !status.operation || busy) return
    creationRunning.current = true
    setCreationBusy(true)
    setCreationError('')
    setCreationNotice('')
    const operation = status.operation
    const selectionRequest = detailRequest.current
    const receipt = receiptRef.current?.operation === operation ? receiptRef.current : null
    try {
      const entity = await guiRetryCreate()
      await acceptCreated(operation, entity, receipt, undefined, selectionRequest)
    } catch (error) {
      await recheckCreation()
      const threadId = receipt?.threadId
      if ((operation === 'post.create' || operation === 'response.create') && isThreadFullFault(error) && threadId && selectedIdRef.current === threadId && selectionRequest === detailRequest.current) {
        setThreadPage((current) => ({ ...current, totalPosts: Math.max(1000, current.totalPosts) }))
        setCreationError('')
      } else if ((operation === 'post.create' || operation === 'response.create') && isThreadFullFault(error)) setCreationError(pendingThreadLimitLabel(language))
      else setCreationError(t.creationFailed)
    } finally {
      creationRunning.current = false
      setCreationBusy(false)
    }
  }

  const discardCreate = async () => {
    if (creationRunning.current || !creationStatusRef.current.pending || busy) return
    creationRunning.current = true
    setCreationBusy(true)
    setCreationError('')
    try {
      const status = await guiDiscardCreate()
      receiptRef.current = null
      creationStatusRef.current = status
      setCreationStatus(status)
      setCreationCheck('ready')
      setCreationNotice('')
    } catch {
      await recheckCreation()
      setCreationError(t.creationUnavailable)
    } finally {
      creationRunning.current = false
      setCreationBusy(false)
    }
  }

  const canCreate = creationCheck === 'ready' && !creationStatus.pending && !creationBusy && !busy
  const selectedCapabilities = capabilityState === 'ready'
    ? capabilities?.threads.find((item) => item.id === currentThread?.id)
    : undefined
  const canCreateBoard = capabilityState === 'ready' && capabilities?.board_create === true
  const allowedThreadBoardIds = capabilityState === 'ready'
    ? new Set(capabilities?.boards.filter((item) => item.thread_create).map((item) => item.id) || [])
    : new Set<string>()
  const allowedThreadBoards = boards.filter((board) => allowedThreadBoardIds.has(board.id))
  const canCreateThread = capabilityState === 'ready' && allowedThreadBoards.length > 0
  const canCreatePost = capabilityState === 'ready' && selectedCapabilities?.post_create === true
  const canCreateResponse = capabilityState === 'ready' && selectedCapabilities?.response_create === true
  const selectedArchived = currentThread?.archived === true || threads.some(thread => thread.id === currentThread?.id && thread.archived === true)
  const threadAtLimit = selectedArchived || threadPage.totalPosts >= 1000
  useEffect(() => {
    if (selectedArchived) setActiveWorkspaceTab(current => current === 'boards' ? 'archive' : current)
  }, [selectedArchived, selectedId])
  const canCreateThreadPost = canCreate && !threadAtLimit
  const canEditPost = capabilityState === 'ready' && selectedCapabilities?.post_edit === true && !creationStatus.pending && !creationBusy && !busy && !postEditBusy
  const canReadPostHistory = capabilityState === 'ready' && selectedCapabilities?.post_history === true
  const canManage = (target: ManagementTarget) => {
    if (capabilityState !== 'ready' || !capabilities || busy || creationCheck !== 'ready' || creationStatus.pending || creationBusy || creationRunning.current || postEditBusy || postEditRunning.current || managementBusy || managementRunning.current || managementNeedsRefresh) return false
    if (target.kind === 'board') return target.action === 'delete'
      ? boards.some((item) => item.id === target.id) && capabilities.boards.some((item) => item.id === target.id && item.board_delete)
      : target.action === 'purge'
        ? deletedItems?.boards.some((item) => item.id === target.id && item.can_purge === true) === true
        : deletedItems?.boards.some((item) => item.id === target.id && item.can_restore !== false) === true
    if (target.kind === 'thread') return target.action === 'delete'
      ? threads.some((item) => item.id === target.id) && capabilities.threads.some((item) => item.id === target.id && item.thread_delete)
      : target.action === 'purge'
        ? deletedItems?.threads.some((item) => item.id === target.id && item.can_purge === true) === true
        : deletedItems?.threads.some((item) => item.id === target.id && item.can_restore !== false && item.parent_deleted !== true) === true
    const post = posts.find((item) => item.id === target.id && item.thread_id === target.threadId && item.no === target.no)
    const threadCapability = capabilities.threads.find((item) => item.id === target.threadId)
    return !!post && post.deleted === (target.action === 'restore') && (target.action !== 'restore' || post.restore_available !== false) && (target.action === 'delete' ? threadCapability?.post_delete : threadCapability?.post_restore) === true
  }

  const openManagement = (target: ManagementTarget, opener: HTMLButtonElement) => {
    if (managementRunning.current || !canManage(target)) return
    managementOpener.current = opener
    setManagementError('')
    setManagementTarget(target)
  }

  const openDeletedItems = (opener: HTMLButtonElement) => {
    if (capabilityState !== 'ready' || creationCheck !== 'ready' || busy || creationStatus.pending || creationBusy || creationRunning.current || postEditBusy || postEditRunning.current || managementRunning.current || managementNeedsRefresh) return
    deletedOpener.current = opener
    setDeletedOpen(true)
    setDeletedItems(null)
    setDeletedSelectionMode(false)
    setSelectedDeletedKeys(new Set())
    setDeletedState('loading')
    void loadDeletedItems(true)
  }

  const manageTarget = async () => {
    const target = managementTarget
    if (!target || managementRunning.current || !canManage(target)) return
    managementRunning.current = true
    setManagementBusy(true)
    setManagementError('')
    const tree = treeRequest.current
    const detail = detailRequest.current
    const permission = capabilityRequest.current
    const selected = selectedIdRef.current
    const contextIsFresh = () => tree === treeRequest.current && detail === detailRequest.current && permission === capabilityRequest.current && selected === selectedIdRef.current && !busy
    const operation: ManageOperation = `${target.kind}.${target.action}` as ManageOperation
    const args = target.kind === 'post'
      ? { thread_id: target.threadId, no: target.no }
      : target.action === 'purge' ? { id: target.id, confirmed: true as const } : { id: target.id }
    try {
      await guiManage(operation, args)
      const fresh = contextIsFresh()
      if (!fresh) {
        setManagementNeedsRefresh(true)
        notifyActivityMutation()
        return
      }
      setManagementTarget(null)
      setManagementError('')
      const refreshed = await refresh()
      if (!refreshed) setManagementNeedsRefresh(true)
    } catch (error) {
      if (!contextIsFresh()) {
        setManagementNeedsRefresh(true)
        return
      }
      if (isAccessFault(error)) { clearPrivateWorkspace(); return }
      setManagementTarget(null)
      setManagementError(t.managementFailed)
      setManagementNeedsRefresh(true)
      const refreshed = await refresh()
      if (!refreshed) setManagementError(t.managementFailed)
    } finally {
      managementRunning.current = false
      setManagementBusy(false)
    }
  }
  const openBulkManagement = (action: DeletedBulkAction) => {
    if (!deletedItems || managementRunning.current || managementBusy || managementNeedsRefresh || !selectedDeletedKeys.size) return
    const listedKeys = new Set([
      ...deletedItems.boards.map((item) => deletedItemKey({ kind: 'board', id: item.id })),
      ...deletedItems.threads.map((item) => deletedItemKey({ kind: 'thread', id: item.id })),
    ])
    if ([...selectedDeletedKeys].some((key) => !listedKeys.has(key))) { setManagementError(t.staleSelection); return }
    const boardsSelected = new Set([...selectedDeletedKeys].filter((key) => key.startsWith('board:')).map((key) => key.slice('board:'.length)))
    if (action === 'restore') {
      const blockedChild = deletedItems.threads.some((item) => selectedDeletedKeys.has(deletedItemKey({ kind: 'thread', id: item.id })) && item.parent_deleted === true && (!item.board_id || !boardsSelected.has(item.board_id)))
      const blockedPermission = deletedItems.boards.some((item) => selectedDeletedKeys.has(deletedItemKey({ kind: 'board', id: item.id })) && item.can_restore === false)
        || deletedItems.threads.some((item) => selectedDeletedKeys.has(deletedItemKey({ kind: 'thread', id: item.id })) && item.can_restore === false)
      if (blockedChild) { setManagementError(t.parentMustBeSelected); return }
      if (blockedPermission) { setManagementError(t.bulkOperationFailed); return }
    } else {
      const blockedPermission = deletedItems.boards.some((item) => selectedDeletedKeys.has(deletedItemKey({ kind: 'board', id: item.id })) && item.can_purge !== true)
        || deletedItems.threads.some((item) => selectedDeletedKeys.has(deletedItemKey({ kind: 'thread', id: item.id })) && item.can_purge !== true)
      if (blockedPermission) { setManagementError(t.bulkOperationFailed); return }
    }
    setManagementError('')
    setBulkManagementAction(action)
  }
  const manageBulk = async () => {
    const action = bulkManagementAction
    if (!action || !deletedItems || managementRunning.current || managementBusy) return
    const listed = [
      ...deletedItems.boards.filter((item) => selectedDeletedKeys.has(deletedItemKey({ kind: 'board', id: item.id }))).map((item) => ({ kind: 'board' as const, id: item.id })),
      ...deletedItems.threads.filter((item) => selectedDeletedKeys.has(deletedItemKey({ kind: 'thread', id: item.id }))).map((item) => ({ kind: 'thread' as const, id: item.id })),
    ]
    if (listed.length !== selectedDeletedKeys.size || !listed.length) { setManagementError(t.staleSelection); return }
    managementRunning.current = true
    setManagementBusy(true)
    setManagementError('')
    const tree = treeRequest.current
    const permission = capabilityRequest.current
    const contextIsFresh = () => tree === treeRequest.current && permission === capabilityRequest.current && !busy
    try {
      await guiManageConversationBatch({ action, items: listed, confirmed: true })
      if (!contextIsFresh()) { setManagementNeedsRefresh(true); return }
      setBulkManagementAction(null)
      setSelectedDeletedKeys(new Set())
      setDeletedSelectionMode(false)
      const refreshed = await refresh()
      if (!refreshed) setManagementNeedsRefresh(true)
      await loadDeletedItems(true)
    } catch (error) {
      if (isAccessFault(error)) { clearPrivateWorkspace(); return }
      setManagementError(t.bulkOperationFailed)
      setManagementNeedsRefresh(true)
      const refreshed = await refresh()
      if (!refreshed) { setManagementNeedsRefresh(true); await loadDeletedItems(true) }
    } finally {
      managementRunning.current = false
      setManagementBusy(false)
    }
  }
  const composerThreadId = currentThread?.id || ''
  const activeReply = canCreateResponse && replyTarget?.threadId === composerThreadId && posts.some((post) => post.no === replyTarget.no && !post.deleted)
    ? replyTarget.no
    : null
  const composerKey = composerThreadId ? JSON.stringify([composerThreadId, activeReply]) : ''
  const composerBody = composerKey ? drafts[composerKey] || '' : ''
  const composerBytes = new TextEncoder().encode(composerBody).byteLength
  const composerVisible = detailState === 'ready' && currentThread !== null && (canCreatePost || (canCreateResponse && activeReply !== null))

  useLayoutEffect(() => {
    const pane = conversationElement.current?.querySelector<HTMLElement>('.composer-pane')
    const form = pane?.querySelector<HTMLElement>('.composer-panel')
    if (!pane || !form) { setComposerMinimum(COMPOSER_HEIGHT_MIN); return }
    const measure = () => {
      const pixels = (value: string) => Number.parseFloat(value) || 0
      const verticalEdges = (element: HTMLElement) => {
        const style = getComputedStyle(element)
        return pixels(style.paddingTop) + pixels(style.paddingBottom) + pixels(style.borderTopWidth) + pixels(style.borderBottomWidth)
      }
      const children = Array.from(form.children) as HTMLElement[]
      const gap = pixels(getComputedStyle(form).rowGap)
      const content = children.reduce((height, child) => {
        const style = getComputedStyle(child)
        const margins = pixels(style.marginTop) + pixels(style.marginBottom)
        if (child.classList.contains('field')) {
          const label = child.querySelector('span')
          const textarea = child.querySelector('textarea')
          return height + margins + Math.max(pixels(style.minHeight), (label?.getBoundingClientRect().height || 0) + pixels(style.rowGap) + (textarea ? pixels(getComputedStyle(textarea).minHeight) : 60))
        }
        return height + margins + child.getBoundingClientRect().height
      }, 0)
      setComposerMinimum(Math.max(COMPOSER_HEIGHT_MIN, Math.ceil(verticalEdges(pane) + verticalEdges(form) + content + gap * Math.max(0, children.length - 1))))
    }
    measure()
    const observer = typeof ResizeObserver === 'undefined' ? null : new ResizeObserver(measure)
    observer?.observe(form)
    for (const child of Array.from(form.children)) observer?.observe(child)
    return () => observer?.disconnect()
  }, [composerVisible, activeReply, formError, threadAtLimit, language])
  const historyPost = postHistory && postHistory.threadId === currentThread?.id && postHistory.current && !postHistory.current.deleted && postHistory.current.body !== null ? postHistory.current : undefined

  useEffect(() => {
    if (!creationDialog) return
    const permitted = creationDialog === 'board' ? canCreateBoard : canCreateThread
    if (!permitted) { closeCreationDialog(false); return }
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') { closeCreationDialog(); return }
      if (event.key !== 'Tab') return
      const focusable = creationDialogElement.current?.querySelectorAll<HTMLElement>('button:not(:disabled), [href], input:not(:disabled), textarea:not(:disabled), select:not(:disabled), [tabindex]:not([tabindex="-1"])')
      if (!focusable?.length) { event.preventDefault(); return }
      const first = focusable[0]
      const last = focusable[focusable.length - 1]
      if (event.shiftKey && document.activeElement === first) { event.preventDefault(); last.focus() }
      else if (!event.shiftKey && document.activeElement === last) { event.preventDefault(); first.focus() }
    }
    creationDialogWasOpen.current = true
    creationDialogFirstField.current?.focus()
    document.addEventListener('keydown', onKeyDown)
    return () => document.removeEventListener('keydown', onKeyDown)
  }, [canCreateBoard, canCreateThread, closeCreationDialog, creationDialog])

  useEffect(() => {
    if (creationDialog || !creationDialogWasOpen.current) return
    creationDialogWasOpen.current = false
    if (creationDialogOpener.current?.isConnected) creationDialogOpener.current.focus()
    creationDialogOpener.current = null
  }, [creationDialog])

  useEffect(() => {
    if (capabilityState !== 'ready' || !currentThread) return
    if (selectedCapabilities?.post_edit !== true) closePostEditor()
    if (selectedCapabilities?.post_history !== true) closePostHistory()
  }, [capabilityState, closePostEditor, closePostHistory, currentThread, selectedCapabilities?.post_edit, selectedCapabilities?.post_history])

  useEffect(() => {
    if (!postHistory) return
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') closePostHistory()
      if (event.key !== 'Tab') return
      const focusable = historyDialog.current?.querySelectorAll<HTMLElement>('button:not(:disabled), [href], input:not(:disabled), textarea:not(:disabled), select:not(:disabled), [tabindex]:not([tabindex="-1"])')
      if (!focusable?.length) { event.preventDefault(); return }
      const first = focusable[0]
      const last = focusable[focusable.length - 1]
      if (event.shiftKey && document.activeElement === first) { event.preventDefault(); last.focus() }
      else if (!event.shiftKey && document.activeElement === last) { event.preventDefault(); first.focus() }
    }
    historyWasOpen.current = true
    historyCloseButton.current?.focus()
    document.addEventListener('keydown', onKeyDown)
    return () => document.removeEventListener('keydown', onKeyDown)
  }, [postHistory, closePostHistory])

  useEffect(() => {
    if (postHistory || !historyWasOpen.current) return
    historyWasOpen.current = false
    historyOpener.current?.focus()
    historyOpener.current = null
  }, [postHistory])

  useEffect(() => {
    if (!managementTarget && !bulkManagementAction) return
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape' && !managementBusy) { setManagementTarget(null); setBulkManagementAction(null); return }
      if (event.key !== 'Tab') return
      const focusable = managementDialog.current?.querySelectorAll<HTMLElement>('button:not(:disabled), [href], input:not(:disabled), textarea:not(:disabled), select:not(:disabled), [tabindex]:not([tabindex="-1"])')
      if (!focusable?.length) { event.preventDefault(); return }
      const first = focusable[0]
      const last = focusable[focusable.length - 1]
      if (event.shiftKey && document.activeElement === first) { event.preventDefault(); last.focus() }
      else if (!event.shiftKey && document.activeElement === last) { event.preventDefault(); first.focus() }
    }
    managementWasOpen.current = true
    managementConfirmButton.current?.focus()
    document.addEventListener('keydown', onKeyDown)
    return () => document.removeEventListener('keydown', onKeyDown)
  }, [managementBusy, managementTarget, bulkManagementAction])

  useEffect(() => {
    if (managementTarget || bulkManagementAction || !managementWasOpen.current) return
    managementWasOpen.current = false
    if (deletedOpen) deletedCloseButton.current?.focus()
    else if (managementOpener.current?.isConnected) managementOpener.current.focus()
    else document.querySelector<HTMLButtonElement>('.deleted-items-action')?.focus()
    managementOpener.current = null
  }, [managementTarget, bulkManagementAction, deletedOpen])

  useEffect(() => {
    if (!deletedOpen || managementTarget || bulkManagementAction) return
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') { setDeletedOpen(false); return }
      if (event.key !== 'Tab') return
      const focusable = deletedDialog.current?.querySelectorAll<HTMLElement>('button:not(:disabled), [href], input:not(:disabled), textarea:not(:disabled), select:not(:disabled), [tabindex]:not([tabindex="-1"])')
      if (!focusable?.length) { event.preventDefault(); return }
      const first = focusable[0]
      const last = focusable[focusable.length - 1]
      if (event.shiftKey && document.activeElement === first) { event.preventDefault(); last.focus() }
      else if (!event.shiftKey && document.activeElement === last) { event.preventDefault(); first.focus() }
    }
    deletedWasOpen.current = true
    deletedCloseButton.current?.focus()
    document.addEventListener('keydown', onKeyDown)
    return () => document.removeEventListener('keydown', onKeyDown)
  }, [deletedOpen, managementTarget, bulkManagementAction])

  useEffect(() => {
    if (deletedOpen || !deletedWasOpen.current) return
    deletedWasOpen.current = false
    deletedOpener.current?.focus()
    deletedOpener.current = null
    ++deletedRequest.current
  }, [deletedOpen])

  const submitBoard = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault()
    if (!canCreateBoard || creationDialogRef.current !== 'board') return
    if (!boardName.trim()) { setBoardFormError(t.nameRequired); return }
    if (characterCount(boardName.trim()) > BOARD_NAME_LIMIT) { setBoardFormError(t.nameTooLong(BOARD_NAME_LIMIT)); return }
    setBoardFormError('')
    void runCreate('board.create', { name: boardName.trim(), description: boardDescription }, null, creationDialogSession.current)
  }

  const submitThread = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault()
    if (!canCreateThread || creationDialogRef.current !== 'thread' || !allowedThreadBoardIds.has(threadBoardId)) return
    if (!threadName.trim()) { setThreadFormError(t.nameRequired); return }
    if (characterCount(threadName.trim()) > THREAD_NAME_LIMIT) { setThreadFormError(t.nameTooLong(THREAD_NAME_LIMIT)); return }
    if (!threadBoardId || !boards.some((board) => board.id === threadBoardId)) { setThreadFormError(t.chooseBoard); return }
    setThreadFormError('')
    void runCreate('thread.create', { board_id: threadBoardId, name: threadName.trim(), description: threadDescription }, null, creationDialogSession.current)
  }

  const submitPost = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault()
    if (!currentThread || !composerKey || creationRunning.current || threadAtLimit) return
    if (activeReply === null ? !canCreatePost : !canCreateResponse) return
    if (!composerBody.trim()) { setFormError(t.bodyRequired); return }
    if (composerBytes > 1024 * 1024) { setFormError(t.bodyTooLarge); return }
    if (characterCount(composerBody) > BODY_LIMIT) { setFormError(t.bodyTooLong); return }
    if (activeReply !== null && (!posts.some((post) => post.no === activeReply && !post.deleted))) { setFormError(t.replyUnavailable); return }
    const operation: CreateOperation = activeReply === null ? 'post.create' : 'response.create'
    const args = activeReply === null
      ? { thread_id: currentThread.id, body: composerBody }
      : { thread_id: currentThread.id, to: activeReply, body: composerBody }
    const receipt: DraftReceipt = { operation, draftKey: composerKey, body: composerBody, threadId: currentThread.id }
    void runCreate(operation, args, receipt)
  }

  const setComposerBody = (value: string) => {
    if (!composerKey) return
    setDrafts((current) => ({ ...current, [composerKey]: value }))
    setFormError('')
  }

  const archiveThreads = threads.filter(thread => thread.archived === true)
  const grouped = new Map(boards.map((board) => [board.id, threads.filter((thread) => thread.board_id === board.id && !thread.archived)]))
  const detached = threads.filter((thread) => thread.board_id === null && !thread.archived)

  const hasStatusErrors = Boolean(serviceError || layoutStorageError || (currentThread && (threadScrollStatus === 'failed' || threadScrollStorageError || viewPreferenceError)) || creationCheck === 'failed' || creationError || managementError || searchNavigationError || notificationError || conversationNotifications.failed || displayedRead.failed || pinError || (capabilityState === 'failed' && treeState !== 'loading') || treeState === 'failed' || operationStatuses.some((entry) => entry.state === 'failed'))
  const workspaceStatusMessages = <>
      <div className="workspace-service-status" data-testid="workspace-service-status" role="status">
        <span>{language === 'ja' ? 'バックグラウンドサービス' : 'Background service'}: {({ unknown: language === 'ja' ? '状態不明' : 'Unknown', checking: language === 'ja' ? '状態を確認中' : 'Checking status', starting: language === 'ja' ? '準備中' : 'Preparing', stopping: language === 'ja' ? '停止処理中' : 'Stopping', running: language === 'ja' ? '稼働中' : 'Running', stopped: language === 'ja' ? '停止中' : 'Stopped', not_running: language === 'ja' ? '未稼働' : 'Not running' })[serviceState]}</span>
        {onServiceRefresh && <button type="button" onClick={onServiceRefresh} disabled={busy || serviceBusy}>{language === 'ja' ? '状態を確認' : 'Check status'}</button>}
      </div>
      {serviceError && <p className="workspace-error" role="alert">{serviceError}</p>}
      {currentThread && (threadScrollStatus || threadScrollStorageError || viewPreferenceError) && <div className="thread-scroll-status-slot">
        {threadScrollStatus && <div className="thread-scroll-status" data-testid="thread-scroll-status" role={threadScrollStatus === 'failed' ? 'alert' : 'status'}>{threadScrollStatus === 'restoring' ? t.restoringPosition : t.restorePositionFailed}</div>}
        {threadScrollStorageError && <div className="thread-scroll-status" data-testid="thread-scroll-storage-status" role="alert">{threadScrollStorageError === 'read' ? t.scrollStorageReadFailed : t.scrollStorageWriteFailed}{threadScrollStorageError === 'read' && <button type="button" onClick={() => void selectThread(currentThread)}>{language === 'ja' ? '再試行' : 'Retry'}</button>}</div>}
        {viewPreferenceError && <div className="thread-scroll-status" data-testid="conversation-view-preference-status" role="alert">{viewPreferenceError === 'read' ? t.viewPreferenceReadFailed : t.viewPreferenceWriteFailed}{viewPreferenceError === 'read' && <button type="button" onClick={() => setPreferenceRetryToken((value) => value + 1)}>{language === 'ja' ? '再試行' : 'Retry'}</button>}</div>}
      </div>}
      {layoutStorageError && <p className="workspace-status workspace-error layout-preferences-status" data-testid="layout-preferences-status" role="status">{layoutStorageError === 'read' ? t.layoutLoadFailed : t.layoutSaveFailed}{layoutStorageError === 'read' && <> <button type="button" onClick={() => setPreferenceRetryToken((value) => value + 1)}>{language === 'ja' ? '再試行' : 'Retry'}</button></>}</p>}
      {creationCheck === 'loading' && <p className="workspace-status" role="status">{t.checkCreation}…</p>}
      {creationCheck === 'failed' && <p className="workspace-status workspace-error" role="alert">{creationError || t.creationUnavailable}</p>}
      {creationStatus.pending && <section className="creation-pending" role="alert" aria-live="polite">
        <div className="creation-copy">
          <strong>{creationStatus.expired ? t.creationExpired : t.creationPending}</strong>
          {creationStatus.operation && <span>{creationLabel(creationStatus.operation, language)}</span>}
          {!creationStatus.expired && <p>{t.discardCaution}</p>}
        </div>
        <div className="workspace-actions">
          <button className="button button-primary workspace-action" type="button" onClick={() => void retryCreate()} disabled={creationBusy || busy || creationStatus.expired}>{t.retryCreate}</button>
          <button className="button button-secondary workspace-action" type="button" onClick={() => void discardCreate()} disabled={creationBusy || busy}>{t.discardCreate}</button>
        </div>
      </section>}
      {creationError && creationCheck !== 'failed' && <p className="workspace-status workspace-error" role="alert">{creationError}</p>}
      {creationNotice && <p className="workspace-status creation-success" role="status">{creationNotice}</p>}
      {managementNeedsRefresh && <p className="workspace-status" role="status">{t.managementRefreshing}</p>}
      {managementError && <p className="workspace-status workspace-error" role="alert">{managementError}</p>}
      {searchNavigationError && <p className="workspace-status workspace-error" data-testid="search-navigation-error" role="alert">{searchNavigationError}</p>}
      {notificationError && <div className="workspace-status notification-status"><p className="workspace-error" data-testid="notification-error" role="alert">{notificationError}</p><button className="post-link" type="button" data-testid="notification-retry" onClick={() => setNotificationRetryToken((value) => value + 1)}>{language === 'ja' ? '再試行' : 'Retry'}</button></div>}
      {conversationNotifications.failed && <div className="workspace-status notification-status"><p className="workspace-error" role="alert">{language === 'ja' ? '購読・未読を確認できませんでした。変更結果が不明な場合は現在の状態を確認してください。' : 'Could not confirm subscriptions or unread counts. Check the current state if a change could not be confirmed.'}</p><button className="post-link" type="button" onClick={() => void conversationNotifications.refresh()}>{language === 'ja' ? '購読・未読を確認' : 'Check subscriptions and unread'}</button></div>}
      {displayedRead.failed && <div className="workspace-status notification-status"><p className="workspace-error" role="alert">{language === 'ja' ? '表示した投稿の既読を保存できませんでした。' : 'Could not save read status for displayed posts.'}</p><button className="post-link" type="button" onClick={displayedRead.retry}>{language === 'ja' ? '既読保存を再試行' : 'Retry saving read status'}</button></div>}
      {pinError && <p className="workspace-status workspace-error" data-testid="thread-pin-error" role="alert">{pinError}</p>}
      {capabilityState === 'loading' && treeState !== 'loading' && <p className="workspace-status" role="status">{t.checkingPermissions}</p>}
      {capabilityState === 'failed' && treeState !== 'loading' && <p className="workspace-status workspace-error" role="alert">{t.permissionFailed}</p>}
      {capabilityState === 'ready' && !canCreateBoard && !canCreateThread && <p className="workspace-status">{t.noBoardThreadPermission}</p>}
      {treeState === 'loading' && <p className="workspace-status" role="status">{t.loading}</p>}
      {treeState === 'failed' && <p className="workspace-status workspace-error" role="alert">{t.unavailable}</p>}
      <div className="workspace-operation-log" data-testid="workspace-operation-log" role="log" aria-live="polite" aria-label={language === 'ja' ? '処理状況' : 'Operation status'}>
        {[...operationStatuses.filter((entry) => entry.state === 'pending'), ...operationStatuses.filter((entry) => entry.state !== 'pending').reverse()].map((entry) => <div key={entry.id} className={`workspace-operation-${entry.state}`}>{operationStatusLabel(entry, language)}</div>)}
      </div>
  </>

  const statusLogContents = <>
    <button className="workspace-log-toggle" type="button" data-testid="workspace-log-toggle" aria-label={language === 'ja' ? logExpanded ? 'ログを閉じる' : 'ログを開く' : logExpanded ? 'Collapse log' : 'Expand log'} aria-expanded={logExpanded} aria-controls={statusLogId} onClick={() => { layoutEditSequenceRef.current += 1; setLogExpanded((value) => !value) }}>
      <span aria-hidden="true">{logExpanded ? '▾' : '▸'}</span>
      {!logExpanded && hasStatusErrors && <span className="unread-dot workspace-log-error-dot" data-testid="workspace-log-error-dot" role="img" aria-label={language === 'ja' ? 'エラーがあります' : 'Errors present'} />}
    </button>
    <div className="workspace-log-body" id={statusLogId} hidden={!logExpanded} tabIndex={0}>{workspaceStatusMessages}</div>
  </>

  const selectedBoardIds = new Set(deletedItems?.boards.filter((item) => selectedDeletedKeys.has(deletedItemKey({ kind: 'board', id: item.id }))).map((item) => item.id) || [])
  const selectedDeletedRows = deletedItems ? [
    ...deletedItems.boards.filter((item) => selectedDeletedKeys.has(deletedItemKey({ kind: 'board', id: item.id }))).map((item) => ({ kind: 'board' as const, id: item.id, name: item.name, canRestore: item.can_restore !== false, canPurge: item.can_purge === true, parentDeleted: false, boardId: null as string | null })),
    ...deletedItems.threads.filter((item) => selectedDeletedKeys.has(deletedItemKey({ kind: 'thread', id: item.id }))).map((item) => ({ kind: 'thread' as const, id: item.id, name: item.name, canRestore: item.can_restore !== false, canPurge: item.can_purge === true, parentDeleted: item.parent_deleted === true, boardId: item.board_id })),
  ] : []
  const selectedHasStaleRows = selectedDeletedRows.length !== selectedDeletedKeys.size
  const selectedBlockedChild = selectedDeletedRows.some((item) => item.kind === 'thread' && item.parentDeleted && (!item.boardId || !selectedBoardIds.has(item.boardId)))
  const bulkRestoreAllowed = selectedDeletedRows.length > 0 && !selectedHasStaleRows && !selectedBlockedChild && selectedDeletedRows.every((item) => item.canRestore)
  const bulkPurgeAllowed = selectedDeletedRows.length > 0 && !selectedHasStaleRows && selectedDeletedRows.every((item) => item.canPurge)
  const toggleDeletedSelection = (item: DeletedItemRef, checked: boolean) => setSelectedDeletedKeys((current) => {
    const next = new Set(current)
    const key = deletedItemKey(item)
    if (checked) next.add(key); else next.delete(key)
    return next
  })
  const selectAllDeleted = () => setSelectedDeletedKeys(new Set([
    ...(deletedItems?.boards.filter((item) => item.can_restore !== false || item.can_purge === true).map((item) => deletedItemKey({ kind: 'board', id: item.id })) || []),
    ...(deletedItems?.threads.filter((item) => item.can_restore !== false || item.can_purge === true).map((item) => deletedItemKey({ kind: 'thread', id: item.id })) || []),
  ]))

  return (
    <section className="workspace-panel" aria-label={t.boards} style={{ '--tree-width': `${boundedTreeWidth}px` } as CSSProperties}>
      <div className="workspace-background" inert={creationDialog !== null || searchOpen || pendingExternalUrl !== null || undefined} aria-hidden={creationDialog !== null || searchOpen || pendingExternalUrl !== null || undefined}>
      <header className="workspace-toolbar" aria-label={t.boards}>
        <a className="brand workspace-brand" href="#home" aria-label="Swarm home"><img className="brand-icon" src="/icon.png" alt="" /><span>Swarm</span></a>
        <div className="workspace-actions">
          {canCreateBoard && <button className="button button-primary workspace-action" type="button" onClick={(event) => openCreationDialog('board', event.currentTarget)} disabled={!canCreate}>{t.createBoard}</button>}
          {canCreateThread && <button className="button button-primary workspace-action" type="button" onClick={(event) => openCreationDialog('thread', event.currentTarget)} disabled={!canCreate}>{t.createThread}</button>}
          <button className="button button-secondary workspace-action" data-testid="workspace-search-open" type="button" onClick={(event) => openSearchDialog(event.currentTarget)} disabled={treeState !== 'ready' || busy || creationBusy || creationStatus.pending || !!creationDialog || searchOpen || !!postHistory || deletedOpen || !!managementTarget || managementBusy || postEditBusy || postEditRunning.current}>{t.search}</button>
          <button className="button button-secondary workspace-action" type="button" onClick={(event) => onSettings?.(event.currentTarget)} disabled={!onSettings || busy || creationBusy || creationStatus.pending || !!creationDialog || !!postHistory || deletedOpen || !!managementTarget || managementBusy || postEditBusy || postEditRunning.current}>{t.settings}</button>
          <button className={`button button-secondary workspace-action${!currentThread ? ' refresh-empty' : ''}`} data-testid="refresh-current-thread" type="button" aria-disabled={!currentThread || undefined} data-disabled-reason={!currentThread ? refreshCopy[language].noCurrent : undefined} onClick={() => void refreshCurrentThread()} disabled={detailState === 'loading' || treeState === 'loading' || busy}>{refreshCopy[language].current}</button>
          <button className={`button button-secondary workspace-action${treeState === 'ready' && threads.length === 0 ? ' refresh-empty' : ''}`} data-testid="refresh-all-threads" type="button" aria-disabled={treeState === 'ready' && threads.length === 0 || undefined} data-disabled-reason={treeState === 'ready' && threads.length === 0 ? refreshCopy[language].noThreads : undefined} onClick={() => void refreshAllThreads()} disabled={treeState === 'loading' || busy}>{refreshCopy[language].all}</button>
          <button className={`button button-secondary workspace-action${pinnedState === 'ready' && pinnedThreads.length === 0 ? ' refresh-empty' : ''}`} data-testid="refresh-pinned-threads" type="button" aria-disabled={pinnedState === 'ready' && pinnedThreads.length === 0 || undefined} data-disabled-reason={pinnedState === 'ready' && pinnedThreads.length === 0 ? refreshCopy[language].noPins : undefined} onClick={() => void refreshPinnedScope()} disabled={pinnedState === 'loading' || treeState === 'loading' || detailState === 'loading' || busy}>{refreshCopy[language].pinned}</button>
          <button className="button button-secondary workspace-action deleted-items-action" type="button" onClick={(event) => openDeletedItems(event.currentTarget)} disabled={capabilityState !== 'ready' || creationCheck !== 'ready' || busy || creationStatus.pending || creationBusy || managementNeedsRefresh}>{t.deletedItems}</button>
          <button className="button button-secondary workspace-action" type="button" onClick={() => void recheckCreation()} disabled={creationBusy || busy}>{t.checkCreation}</button>
          <button className="button button-secondary workspace-action" type="button" onClick={onLogout} disabled={busy || creationBusy || creationCheck !== 'ready' || creationStatus.pending}>{language === 'ja' ? 'ログアウト' : 'Log out'}</button>
        </div>
      </header>
      {treeState === 'ready' && <div className="master-detail" ref={masterDetailElement} style={{ '--tree-width': `${boundedTreeWidth}px` } as CSSProperties}>
        <div className="workspace-side-pane">
          <div className="workspace-tabs" role="tablist" aria-label={language === 'ja' ? 'ワークスペース' : 'Workspace'}>
            {(['boards', 'pinned', 'archive', 'activity'] as WorkspaceTab[]).map((tab) => <button
              className="workspace-tab"
              data-testid={`workspace-tab-${tab}`}
              data-workspace-tab={tab}
              id={`workspace-tab-${tab}`}
              type="button"
              role="tab"
              aria-selected={activeWorkspaceTab === tab}
              aria-controls={`workspace-panel-${tab}`}
              tabIndex={activeWorkspaceTab === tab ? 0 : -1}
              ref={(element) => { workspaceTabsRef.current[tab] = element }}
              onClick={() => setActiveWorkspaceTab(tab)}
              onKeyDown={handleWorkspaceTabKeyDown}
              key={tab}
            >{workspaceTabLabel(language, tab)}</button>)}
          </div>
          <section className="workspace-tab-panel" id="workspace-panel-boards" data-testid="workspace-panel-boards" role="tabpanel" aria-labelledby="workspace-tab-boards" tabIndex={0} hidden={activeWorkspaceTab !== 'boards'}>
        <nav className="thread-tree" aria-label={t.boards}>
          {boards.length === 0 && detached.length === 0 && <p className="tree-empty">{t.noBoards}</p>}
          {boards.map((board) => {
            const entries = grouped.get(board.id) || []
            const boardUnread = entries.reduce((total, thread) => total + unreadCount(thread.id), 0)
            const canDeleteBoard = capabilityState === 'ready' && capabilities?.boards.some((item) => item.id === board.id && item.board_delete) === true
            return <div className="tree-board" key={board.id}>
              <div className="tree-board-heading">
                <button className={`tree-board-toggle${searchHighlight.has(`board:${board.id}`) ? ' search-target-highlight' : ''}`} type="button" ref={(element) => { boardToggleRefs.current[board.id] = element }} aria-expanded={!collapsedBoards.has(board.id)} aria-controls={`board-threads-${board.id}`} onClick={() => setCollapsedBoards((current) => {
                  const next = new Set(current)
                  if (next.has(board.id)) next.delete(board.id)
                  else next.add(board.id)
                  return next
                })}>
                  <svg className="tree-folder-icon" viewBox="0 0 20 16" aria-hidden="true"><path d="M1.5 3.5h6l1.8 2h9.2v8.7a1.3 1.3 0 0 1-1.3 1.3H2.8a1.3 1.3 0 0 1-1.3-1.3z" /><path d="M1.5 5.5V2.8a1.3 1.3 0 0 1 1.3 1.3h4.6l1.8 2h7.9a1.3 1.3 0 0 1 1.3 1.3v.7" /></svg><span className="tree-board-title">{board.name}</span>{unreadDot(boardUnread)}
                </button>
                {canDeleteBoard && <button className="post-link tree-manage-action" type="button" onClick={(event) => openManagement({ kind: 'board', id: board.id, name: board.name, action: 'delete' }, event.currentTarget)} disabled={!canManage({ kind: 'board', id: board.id, name: board.name, action: 'delete' })}>{t.delete}</button>}
              </div>
              <div className="tree-board-content" id={`board-threads-${board.id}`} hidden={collapsedBoards.has(board.id)}>
              {entries.length ? <div className="tree-threads" role="group" aria-label={board.name}>
                {entries.map((thread) => {
                  const target: ManagementTarget = { kind: 'thread', id: thread.id, name: thread.name, action: 'delete', boardName: board.name }
                  const canDeleteThread = capabilityState === 'ready' && capabilities?.threads.some((item) => item.id === thread.id && item.thread_delete) === true
                  return <div className="tree-thread-row" key={thread.id}><button type="button" ref={selectedId === thread.id ? selectedThreadButton : undefined} className={`tree-thread${selectedId === thread.id ? ' is-selected' : ''}${searchHighlight.has(`thread:${thread.id}`) ? ' search-target-highlight' : ''}`} aria-current={selectedId === thread.id ? 'page' : undefined} onClick={() => void selectThread(thread)}><svg className="tree-thread-icon" viewBox="0 0 18 18" aria-hidden="true"><path d="M3 3.5h12a1.5 1.5 0 0 1 1.5 1.5v7a1.5 1.5 0 0 1-1.5 1.5H8l-4.5 2v-2.1A1.5 1.5 0 0 1 2 12V5a1.5 1.5 0 0 1 1-1.5Z" /></svg><span className="tree-thread-title">{thread.name}</span>{unreadBadge(thread.id)}</button>{canDeleteThread && <button className="post-link tree-manage-action" type="button" onClick={(event) => openManagement(target, event.currentTarget)} disabled={!canManage(target)}>{t.delete}</button>}</div>
                })}
              </div> : <p className="tree-empty tree-nested-empty">{t.noThreads}</p>}
              </div>
            </div>
          })}
          {detached.length > 0 && <section className="tree-unparented" aria-labelledby="accessible-title">
            <h2 id="accessible-title">{t.accessible}</h2>
            <div className="tree-threads" role="group" aria-label={t.accessible}>
              {detached.map((thread) => {
                const target: ManagementTarget = { kind: 'thread', id: thread.id, name: thread.name, action: 'delete', boardName: null }
                const canDeleteThread = capabilityState === 'ready' && capabilities?.threads.some((item) => item.id === thread.id && item.thread_delete) === true
                return <div className="tree-thread-row" key={thread.id}><button type="button" ref={selectedId === thread.id ? selectedThreadButton : undefined} className={`tree-thread${selectedId === thread.id ? ' is-selected' : ''}${searchHighlight.has(`thread:${thread.id}`) ? ' search-target-highlight' : ''}`} aria-current={selectedId === thread.id ? 'page' : undefined} onClick={() => void selectThread(thread)}><span className="tree-thread-title">{thread.name}</span>{unreadBadge(thread.id)}</button>{canDeleteThread && <button className="post-link tree-manage-action" type="button" onClick={(event) => openManagement(target, event.currentTarget)} disabled={!canManage(target)}>{t.delete}</button>}</div>
              })}
            </div>
          </section>}
        </nav>
          </section>
          <section className="workspace-tab-panel workspace-list-panel" id="workspace-panel-pinned" data-testid="workspace-panel-pinned" role="tabpanel" aria-labelledby="workspace-tab-pinned" tabIndex={0} hidden={activeWorkspaceTab !== 'pinned'}>
            {pinnedState === 'loading' && <p className="tree-empty" data-testid="workspace-pins-loading" role="status">{language === 'ja' ? 'ピン留めしたThreadを読み込み中…' : 'Loading pinned Threads…'}</p>}
            {pinnedState === 'failed' && <div className="workspace-side-error"><p className="workspace-error" data-testid="workspace-pins-error" role="alert">{pinReadFailedLabel(language)}</p><button className="post-link" data-testid="workspace-pins-retry" type="button" onClick={() => void refreshPinnedThreads()}>{language === 'ja' ? '再試行' : 'Retry'}</button></div>}
            {pinnedState === 'ready' && pinnedThreads.length === 0 && <p className="tree-empty">{language === 'ja' ? 'ピン留めしたThreadはありません。' : 'No pinned Threads.'}</p>}
            {pinnedState === 'ready' && pinnedThreads.length > 0 && <div className="tree-threads pinned-thread-list" role="list" aria-label={workspaceTabLabel(language, 'pinned')}>
              {pinnedThreads.map((thread) => {
                const target: ManagementTarget = { kind: 'thread', id: thread.id, name: thread.name, action: 'delete', boardName: thread.board_id ? boards.find((item) => item.id === thread.board_id)?.name || null : null }
                const canDeleteThread = capabilityState === 'ready' && capabilities?.threads.some((item) => item.id === thread.id && item.thread_delete) === true
                return <div className="tree-thread-row" role="listitem" key={thread.id}><button type="button" ref={selectedId === thread.id ? selectedThreadButton : undefined} className={`tree-thread${selectedId === thread.id ? ' is-selected' : ''}`} aria-current={selectedId === thread.id ? 'page' : undefined} onClick={() => void selectThread(thread)}><span className="tree-thread-title">{thread.name}</span>{unreadBadge(thread.id)}</button>{canDeleteThread && <button className="post-link tree-manage-action" type="button" onClick={(event) => openManagement(target, event.currentTarget)} disabled={!canManage(target)}>{t.delete}</button>}</div>
              })}
            </div>}
          </section>
          <section className="workspace-tab-panel workspace-list-panel" id="workspace-panel-archive" data-testid="workspace-panel-archive" role="tabpanel" aria-labelledby="workspace-tab-archive" tabIndex={0} hidden={activeWorkspaceTab !== 'archive'}>
            {treeState === 'ready' && archiveThreads.length === 0 && <p className="tree-empty">{language === 'ja' ? '過去スレッドはありません。' : 'No archived Threads.'}</p>}
            {treeState === 'ready' && <div className="tree-threads" role="list" aria-label={workspaceTabLabel(language, 'archive')}>
              {archiveThreads.map(thread => {
                const boardName = thread.board_id ? boards.find(board => board.id === thread.board_id)?.name : null
                return <div className="tree-thread-row" role="listitem" key={thread.id}>
                  <button type="button" ref={selectedId === thread.id ? selectedThreadButton : undefined} className={`tree-thread${selectedId === thread.id ? ' is-selected' : ''}${searchHighlight.has(`thread:${thread.id}`) ? ' search-target-highlight' : ''}`} aria-current={selectedId === thread.id ? 'page' : undefined} onClick={() => void selectThread(thread)} title={thread.name}>
                    <span className="archive-thread-copy"><span className="tree-thread-title">{thread.name}</span>{boardName && <span className="archive-board-name">{boardName}</span>}</span>{unreadBadge(thread.id)}
                  </button>
                </div>
              })}
            </div>}
          </section>
          <section className="workspace-tab-panel workspace-activity-panel" id="workspace-panel-activity" data-testid="workspace-panel-activity" role="tabpanel" aria-labelledby="workspace-tab-activity" tabIndex={0} hidden={activeWorkspaceTab !== 'activity'}>
            {activityState === 'loading' && <p className="tree-empty" data-testid="workspace-activity-loading" role="status">{activityLoadingLabel(language)}</p>}
            {activityState === 'failed' && <div className="workspace-side-error"><p className="workspace-error" data-testid="workspace-activity-error" role="alert">{activityReadFailedLabel(language)}</p><button className="post-link" data-testid="workspace-activity-retry" type="button" onClick={() => void refreshActivity()}>{language === 'ja' ? '再試行' : 'Retry'}</button></div>}
            {activityState === 'ready' && <>
              {activityItems.length > 0
                ? <ol className="workspace-activity-list" data-testid="workspace-activity-list" aria-label={workspaceTabLabel(language, 'activity')}>
                    {activityItems.map((item) => <li className="workspace-activity-item" key={item.id}>
                      <time dateTime={item.occurred_at}>{localizedDate(item.occurred_at, language)}</time>
                      <p>{activityActionLabel(item.action, language)}</p>
                      {(item.board_name || item.thread_name || item.post_no !== null) && <span className="workspace-activity-context">{[item.board_name, item.thread_name, item.post_no !== null ? `No. ${item.post_no}` : null].filter(Boolean).join(' · ')}</span>}
                    </li>)}
                  </ol>
                : activityNextId === null && <p className="tree-empty">{activityEmptyLabel(language)}</p>}
              {activityError && <div className="workspace-side-error"><p className="workspace-error" data-testid="workspace-activity-error" role="alert">{activityReadFailedLabel(language)}</p><button className="post-link" data-testid="workspace-activity-retry" type="button" onClick={() => void refreshActivity()}>{language === 'ja' ? '再試行' : 'Retry'}</button></div>}
              {activityNextId !== null && <button className="button button-secondary activity-load-more" data-testid="workspace-activity-load-more" type="button" onClick={() => void loadMoreActivity()} disabled={activityLoadingMore}>{activityLoadingMore ? activityLoadingLabel(language) : language === 'ja' ? 'さらに読み込む' : 'Load more'}</button>}
            </>}
          </section>
          <footer className="workspace-counts" data-testid="workspace-counts" aria-label={language === 'ja' ? '閲覧可能な件数' : 'Visible counts'}>
            <span>{language === 'ja' ? 'ボード' : 'Boards'}: {boards.length}</span>
            <span>{language === 'ja' ? 'スレッド' : 'Threads'}: {threads.length - archiveThreads.length}</span>
            <span>{language === 'ja' ? '過去スレッド' : 'Archive'}: {archiveThreads.length}</span>
            <span>{language === 'ja' ? '未読投稿' : 'Unread posts'}: {conversationNotifications.ready ? conversationNotifications.unread.total : '—'}</span>
          </footer>
        </div>
        <div className="resize-separator tree-resize-separator" data-testid="tree-width-separator" role="separator" aria-orientation="vertical" aria-label={t.resizeBoards} aria-valuemin={220} aria-valuemax={treeWidthMaximum} aria-valuenow={boundedTreeWidth} tabIndex={0} onKeyDown={handleTreeResizeKeyDown} onPointerDown={handleTreeResizePointerDown} onPointerMove={handleTreeResizePointerMove} onPointerUp={endTreeResize} onPointerCancel={endTreeResize} />
        <div className={`conversation-column${composerVisible ? ' has-composer' : ''}`} ref={conversationElement} style={{ '--composer-min-height': `${composerMinimum}px`, '--composer-height': `${boundedComposerHeight}px` } as CSSProperties}>
        <section className={`thread-detail${!selectedId && !availableUpdate && updateNotes ? ' has-update-notes' : ''}`} ref={threadScrollElement} onScroll={handleThreadScroll} data-testid={currentThread ? 'thread-scroll-container' : undefined} aria-label={currentThread?.name || t.boards}>
          {!selectedId && <section className={`workspace-empty conversation-empty${!availableUpdate && updateNotes ? ' has-update-notes' : ''}`} data-testid="thread-empty-state" aria-labelledby={!availableUpdate && updateNotes ? 'update-notes-title' : 'thread-empty-title'}>
            {!availableUpdate && updateNotes ? <UpdateReleaseNotes notes={updateNotes} language={language} /> : <>
            <img className="workspace-empty-icon" src="/icon.png" alt="" />
            <h2 id="thread-empty-title">{t.quickStartTitle}</h2>
            <p>{t.quickStartIntro}</p>
            <ol className="quick-start-list"><li>{t.quickStartStepOne}</li><li>{t.quickStartStepTwo}</li><li>{t.quickStartStepThree}</li></ol>
            {availableUpdate && <UpdateNotice update={availableUpdate} language={language} />}
            </>}
          </section>}
          {detailState === 'loading' && <p className="detail-placeholder" role="status">{t.threadLoading}</p>}
          {detailState === 'failed' && <p className="detail-placeholder workspace-error" role="alert">{t.threadFailed}</p>}
          {selectedId && !currentThread && <button className="icon-button close-thread-button loading-close" type="button" aria-label={t.closeThread} onClick={closeThread}>×</button>}
          {detailState === 'ready' && currentThread && <>
            <header className="thread-heading">
              <div className="thread-heading-copy"><h1 className={searchHighlight.has(`thread:${currentThread.id}`) ? 'search-target-highlight' : undefined} title={currentThread.name}>{currentThread.name}</h1>{currentThread.description && <p>{currentThread.description}</p>}</div>
              <div className="thread-heading-toolbar">
                <div className="conversation-view-switch" data-testid="conversation-view-switch" role="group" aria-label={t.conversationViewLabel}>
                  <button className="conversation-view-option" data-testid="conversation-view-timeline" type="button" aria-pressed={conversationView === 'timeline'} onClick={() => changeConversationView('timeline')} disabled={paging}>{t.timelineView}</button>
                  <button className="conversation-view-option" data-testid="conversation-view-threaded" type="button" aria-pressed={conversationView === 'threaded'} onClick={() => changeConversationView('threaded')} disabled={paging}>{t.threadedView}</button>
                </div>
              <div className="thread-heading-actions">
                <button className={`icon-button thread-subscribe-button${conversationNotifications.subscribed.has(currentThread.id) ? ' is-subscribed' : ''}`} data-testid="thread-subscribe-button" type="button" aria-pressed={conversationNotifications.subscribed.has(currentThread.id)} aria-label={language === 'ja' ? conversationNotifications.subscribed.has(currentThread.id) ? 'Threadの購読を解除' : 'Threadを購読' : conversationNotifications.subscribed.has(currentThread.id) ? 'Unsubscribe from Thread' : 'Subscribe to Thread'} title={language === 'ja' ? 'このThreadの新着通知を購読' : 'Subscribe to new posts in this Thread'} disabled={!conversationNotifications.ready || conversationNotifications.saving || busy || creationBusy || creationStatus.pending} onClick={() => void conversationNotifications.toggle(currentThread.id)}><svg viewBox="0 0 20 20" aria-hidden="true"><path d="M4 14h12l-2-3V7a4 4 0 0 0-8 0v4l-2 3ZM8 16a2 2 0 0 0 4 0" /></svg></button>
                <button className={`icon-button thread-pin-button${pinnedThreads.some((item) => item.id === currentThread.id) ? ' is-pinned' : ''}`} data-testid="thread-pin-button" type="button" aria-label={pinThreadLabel(language, pinnedThreads.some((item) => item.id === currentThread.id))} title={pinThreadLabel(language, pinnedThreads.some((item) => item.id === currentThread.id))} aria-pressed={pinnedThreads.some((item) => item.id === currentThread.id)} aria-busy={pinBusyId === currentThread.id || undefined} disabled={pinnedState !== 'ready' || pinBusyId !== null} onClick={() => void toggleThreadPin()}>
                  <svg viewBox="0 0 20 20" aria-hidden="true"><path d="M7 2.5h6l-.8 4 2.2 2.2v1.1H5.6V8.7L7.8 6.5 7 2.5Z" /><path d="M10 9.8v7.7M7 2.5h6" /></svg>
                </button>
                {pinBusyId === currentThread.id && <span className="sr-only" data-testid="thread-pin-saving" role="status">{language === 'ja' ? 'ピン留めを保存中…' : 'Saving pin…'}</span>}
                <button className="icon-button thread-jump-button" data-testid="thread-page-top-button" type="button" aria-label={t.pageTop} title={t.pageTop} onClick={() => jumpToPageEdge('top')}>↑</button>
                <button className="icon-button thread-jump-button" data-testid="thread-page-bottom-button" type="button" aria-label={t.pageBottom} title={t.pageBottom} onClick={() => jumpToPageEdge('bottom')}>↓</button>
                {selectedCapabilities?.thread_delete && <button className="button button-secondary" type="button" onClick={(event) => openManagement({ kind: 'thread', id: currentThread.id, name: currentThread.name, action: 'delete', boardName: currentThread.board_id ? boards.find((item) => item.id === currentThread.board_id)?.name || null : null }, event.currentTarget)} disabled={!canManage({ kind: 'thread', id: currentThread.id, name: currentThread.name, action: 'delete', boardName: null })}>{t.delete}</button>}
                <button className="icon-button close-thread-button" type="button" aria-label={t.closeThread} onClick={closeThread}>×</button>
              </div>
              </div>
            </header>
            {threadAtLimit && !composerVisible && <p className="workspace-status thread-post-limit" data-testid="thread-post-limit-status" role="status">{threadLimitLabel(language)}</p>}
            {posts.length === 0 ? <p className="detail-placeholder">{t.noPosts}</p> : <div className={`timeline${conversationView === 'threaded' ? ' is-threaded' : ''}`} aria-label={currentThread.name}>
              {displayedConversationRows.map(({ post, depth, descendantCount, parentOutsidePage }) => {
                const isCollapsed = collapsedConversationPosts.has(post.no)
                const threaded = conversationView === 'threaded'
                return <article className={`timeline-post${post.deleted ? ' is-deleted' : ''}${threaded ? ' threaded-post' : ''}${!post.deleted && post.body !== null && searchHighlight.has(`post:${post.id}`) ? ' search-target-highlight' : ''}`} id={`post-${post.no}`} data-testid={threaded ? `threaded-post-${post.no}` : undefined} data-post-no={post.no} data-post-id={post.deleted || post.body === null ? undefined : post.id} data-depth={threaded ? depth : undefined} data-descendant-count={threaded ? descendantCount : undefined} style={threaded ? { '--thread-depth': Math.min(depth, 3) } as CSSProperties : undefined} key={post.no}>
                <header className="post-meta">
                  {threaded && depth > 0 && <span className="threaded-depth-label">{t.replyDepth(depth)}</span>}
                  {threaded && parentOutsidePage && <span className="thread-parent-outside-page">{t.parentOutsidePage(post.reply_to!)}</span>}
                  <strong>{authorLabel(post, language)}</strong><time dateTime={post.created_at}>{localizedDate(post.created_at, language)}</time>{post.edited_by && <span className="edited-mark">{t.edited}{post.editor_display_id ? ` · ${post.editor_display_id}` : ''}</span>}
                  {threaded && descendantCount > 0 && <button className="thread-subtree-toggle" data-testid={`threaded-collapse-${post.no}`} type="button" aria-expanded={!isCollapsed} aria-label={isCollapsed ? t.expandReplies(descendantCount) : t.collapseReplies(descendantCount)} onClick={() => toggleConversationPost(post.no)}>{isCollapsed ? '+' : '−'} <span>{descendantCount}</span></button>}
                </header>
                {post.deleted ? <p className="post-deleted">{t.deleted}{post.restore_available === false ? language === 'ja' ? ' — 保持期限を過ぎたため復元できません。' : ' — Retention expired; restoration unavailable.' : ''}</p> : <MarkdownBody language={language} onOpenLink={requestExternalOpen}>{post.body}</MarkdownBody>}
                <footer className="post-relations">
                  {post.reply_to !== null && <button className="post-link" type="button" onClick={() => void followReply(post.reply_to!)}>{t.replyTo} No. {post.reply_to}</button>}
                  {post.response_count > 0 && <button className="response-badge" type="button" aria-label={`${post.response_count >= 99 ? '99+' : post.response_count} ${t.responses}`} onClick={() => void openResponses(post)}>{post.response_count >= 99 ? '99+' : post.response_count} {t.responses}</button>}
                  {!post.deleted && canCreateResponse && <button className="post-link reply-action" type="button" disabled={!canCreateThreadPost} onClick={() => { setReplyTarget({ threadId: currentThread.id, no: post.no }); setFormError('') }}>{t.sendResponse} · No. {post.no}</button>}
                  {!post.deleted && post.body !== null && canEditPost && <button className="post-link post-tool-action" type="button" aria-label={`${t.edit} No. ${post.no}`} onClick={() => editPost(post)}>{t.edit}</button>}
                  {!post.deleted && post.body !== null && canReadPostHistory && <button className="post-link post-tool-action" type="button" aria-label={`${t.history} No. ${post.no}`} onClick={(event) => void openPostHistory(post, event.currentTarget)}>{t.history}</button>}
                  {post.deleted && post.restore_available !== false && selectedCapabilities?.post_restore && <button className="post-link post-tool-action" type="button" aria-label={`${t.restore} No. ${post.no}`} onClick={(event) => openManagement({ kind: 'post', id: post.id, threadId: currentThread.id, threadName: currentThread.name, no: post.no, action: 'restore' }, event.currentTarget)} disabled={!canManage({ kind: 'post', id: post.id, threadId: currentThread.id, threadName: currentThread.name, no: post.no, action: 'restore' })}>{t.restore}</button>}
                  {!post.deleted && post.body !== null && selectedCapabilities?.post_delete && <button className="post-link post-tool-action" type="button" aria-label={`${t.delete} No. ${post.no}`} onClick={(event) => openManagement({ kind: 'post', id: post.id, threadId: currentThread.id, threadName: currentThread.name, no: post.no, action: 'delete' }, event.currentTarget)} disabled={!canManage({ kind: 'post', id: post.id, threadId: currentThread.id, threadName: currentThread.name, no: post.no, action: 'delete' })}>{t.delete}</button>}
                </footer>
                {postEditor?.threadId === currentThread.id && postEditor.no === post.no && <form className="post-edit-panel" aria-label={`${t.editPost} No. ${post.no}`} onSubmit={(event) => void submitPostEdit(event)}>
                  <h2>{t.editPost} · No. {post.no}</h2>
                  {postEditor.latest && postEditor.reviewedLatest && <section className="latest-version" aria-label={t.latestVersion}>
                    <h3>{t.latestVersion} · revision {postEditor.latest.revision}</h3>
                    <MarkdownBody language={language} onOpenLink={requestExternalOpen}>{postEditor.latest.body}</MarkdownBody>
                  </section>}
                  <label className="field"><span>{t.editBody}</span><textarea value={postEditor.draft} onChange={(event) => setPostEditor({ ...postEditor, draft: event.target.value, error: '' })} disabled={postEditor.saving || postEditor.reloading || creationStatus.pending || creationBusy || busy} rows={5} /></label>
                  <div className="composer-footer"><span className={`byte-count${characterCount(postEditor.draft) > BODY_LIMIT ? ' is-over-limit' : ''}`} role="status">{characterCounter(postEditor.draft, BODY_LIMIT, language)}</span><div className="post-edit-actions">
                    {postEditor.conflict && <button className="button button-secondary" type="button" onClick={() => void reloadLatestPost()} disabled={postEditor.reloading || postEditor.saving || creationStatus.pending || creationBusy || busy}>{postEditor.reloading ? t.reloadingLatest : t.reloadLatest}</button>}
                    <button className="button button-secondary" type="button" onClick={closePostEditor} disabled={postEditor.saving}>{t.cancel}</button>
                    <button className="button button-primary" type="submit" disabled={!canEditPost || postEditor.saving || postEditor.reloading || (postEditor.conflict && !postEditor.reviewedLatest)}>{postEditor.saving ? t.savingEdit : t.saveEdit}</button>
                  </div></div>
                  {postEditor.error && <p className="form-error" role="alert">{postEditor.error}</p>}
                </form>}
              </article>
              })}
            </div>}
            {pagingError && <p className="workspace-error" role="alert">{pageLoadFailedLabel(language)}</p>}
            <nav className="thread-pagination" aria-label={pageControlLabel(language, 'navigation')}>
              {paging && <span className="page-loading-status" data-testid="thread-page-loading" role="status">{pageLoadingLabel(language)}</span>}
              <div className="thread-page-buttons">
                <button className="page-nav-button" data-testid="thread-page-first-button" type="button" aria-label={pageControlLabel(language, 'first')} title={pageControlLabel(language, 'first')} onClick={() => void requestThreadPage(1)} disabled={paging || postEditBusy || postEditRunning.current || threadPage.page <= 1}>«</button>
                <button className="page-nav-button" data-testid="thread-page-previous-button" type="button" aria-label={pageControlLabel(language, 'previous')} title={pageControlLabel(language, 'previous')} onClick={() => void requestThreadPage(threadPage.page - 1)} disabled={paging || postEditBusy || postEditRunning.current || threadPage.page <= 1}>‹</button>
                {pageNumberItems(threadPage.page, threadPage.totalPages).map((item, index) => item === 'ellipsis'
                  ? <span className="page-ellipsis" aria-hidden="true" key={`ellipsis-${index}`}>…</span>
                  : <button className="page-nav-button page-number-button" data-testid={`thread-page-number-${item}`} type="button" aria-label={language === 'ja' ? `${item}ページ` : `Page ${item}`} aria-current={item === threadPage.page ? 'page' : undefined} onClick={() => item !== threadPage.page && void requestThreadPage(item)} disabled={paging || postEditBusy || postEditRunning.current}>{item}</button>)}
                <button className="page-nav-button" data-testid="thread-page-next-button" type="button" aria-label={pageControlLabel(language, 'next')} title={pageControlLabel(language, 'next')} onClick={() => void requestThreadPage(threadPage.page + 1)} disabled={paging || postEditBusy || postEditRunning.current || threadPage.page >= threadPage.totalPages}>›</button>
                <button className="page-nav-button" data-testid="thread-page-last-button" type="button" aria-label={pageControlLabel(language, 'last')} title={pageControlLabel(language, 'last')} onClick={() => void requestThreadPage(threadPage.totalPages)} disabled={paging || postEditBusy || postEditRunning.current || threadPage.page >= threadPage.totalPages}>»</button>
              </div>
              <label className="page-size-control"><span>{pageControlLabel(language, 'size')}</span><select data-testid="thread-page-size-select" aria-label={pageControlLabel(language, 'size')} value={threadPage.pageSize} onChange={(event) => changePageSize(event.target.value)} disabled={paging || postEditBusy || postEditRunning.current}><option value={50}>50</option><option value={100}>100</option><option value={500}>500</option><option value={1000}>1000</option></select></label>
            </nav>
            {(responseState || parentPost) && <section className="related-panel" aria-live="polite" aria-label={t.responses}>
              {responseState === 'loading' && <p>{t.threadLoading}</p>}
              {responseState === 'failed' && <p className="workspace-error">{t.responseFailed}</p>}
              {responses && <><h2>No. {responses.target} · {t.responses}</h2>{responses.items.length ? responses.items.map((response) => <div className="related-post" key={response.no}><strong>{authorLabel(response, language)}</strong><MarkdownBody language={language} onOpenLink={requestExternalOpen}>{response.body}</MarkdownBody></div>) : <p>{t.noResponses}</p>}{responseNext !== null && <button className="button button-secondary older-button" type="button" onClick={() => void loadOlderResponses()} disabled={responsePaging}>{responsePaging ? t.loadingOlder : t.older}</button>}</>}
              {parentPost && <div className="related-post"><strong>{t.replyTo} · {authorLabel(parentPost, language)}</strong>{parentPost.deleted ? <p>{t.deleted}</p> : <MarkdownBody language={language} onOpenLink={requestExternalOpen}>{parentPost.body}</MarkdownBody>}</div>}
            </section>}
            {capabilityState === 'ready' && !canCreatePost && !canCreateResponse && !threadAtLimit && <p className="workspace-status">{t.noCreatePermission}</p>}
            {capabilityState === 'loading' && <p className="workspace-status" role="status">{t.checkingPermissions}</p>}
            {capabilityState === 'failed' && <p className="workspace-status workspace-error" role="alert">{t.permissionFailed}</p>}
          </>}
        </section>
        {composerVisible && <div className="composer-resize-separator" data-testid="composer-height-separator" role="separator" aria-orientation="horizontal" aria-label={t.resizeComposer} aria-valuemin={composerMinimum} aria-valuemax={composerHeightMaximum} aria-valuenow={boundedComposerHeight} tabIndex={0} onKeyDown={handleComposerResizeKeyDown} onPointerDown={handleComposerResizePointerDown} onPointerMove={handleComposerResizePointerMove} onPointerUp={endComposerResize} onPointerCancel={endComposerResize} />}
        {composerVisible && <section className="composer-pane" aria-label={activeReply === null ? t.sendPost : `${t.sendResponse} No. ${activeReply}`}>
          <form className="composer-panel" onSubmit={submitPost}>
            {activeReply !== null && <div className="reply-context"><strong>{t.replyTo} No. {activeReply}</strong><button className="post-link" type="button" onClick={() => { setReplyTarget(null); setFormError('') }} disabled={creationBusy}>{t.cancelReply}</button></div>}
            <label className="field"><span>{activeReply === null ? t.sendPost : `${t.sendResponse} · No. ${activeReply}`}</span>
              <textarea className={threadAtLimit ? 'thread-post-limit' : undefined} data-testid={threadAtLimit ? 'thread-post-limit-status' : undefined} value={threadAtLimit ? `${threadLimitLabel(language)}${composerBody ? `\n\n${composerBody}` : ''}` : composerBody} onChange={(event) => setComposerBody(event.target.value)} placeholder={activeReply === null ? t.postPlaceholder : t.responsePlaceholder} disabled={!canCreateThreadPost} rows={5} />
            </label>
            <div className="composer-footer"><span className={composerBytes > 1024 * 1024 || characterCount(composerBody) > BODY_LIMIT ? 'byte-count is-over-limit' : 'byte-count'} role="status">{characterCounter(composerBody, BODY_LIMIT, language)}</span><button className="button button-primary" type="submit" disabled={!canCreateThreadPost || creationBusy}>{creationBusy ? t.loading : activeReply === null ? t.sendPost : t.sendResponse}</button></div>
            {formError && <p className="form-error" role="alert">{formError}</p>}
          </form>
        </section>}
        <footer className={`workspace-status-footer${logExpanded ? '' : ' is-collapsed'}`} data-testid="workspace-status-footer" aria-label={language === 'ja' ? 'ワークスペースの状態' : 'Workspace status'}>{statusLogContents}</footer>
        </div>
      </div>}
      {treeState !== 'ready' && <footer className={`workspace-status-footer workspace-status-footer-pending${logExpanded ? '' : ' is-collapsed'}`} data-testid="workspace-status-footer" aria-label={language === 'ja' ? 'ワークスペースの状態' : 'Workspace status'}>{statusLogContents}</footer>}
      </div>
      {searchOpen && <SearchDialog
        language={language}
        accountId={accountId}
        boards={boards}
        threads={threads}
        currentThread={currentThread}
        busy={busy || creationBusy}
        creationPending={creationStatus.pending}
        refreshToken={refreshToken}
        openerRef={searchOpener}
        onClose={closeSearchDialog}
        onNavigate={navigateSearchResult}
        initialQuery={searchQuery}
        onQueryChange={setSearchQuery}
        hasHighlight={searchHighlight.size > 0}
        onClearHighlight={() => setSearchHighlight(new Set())}
      />}
      {creationDialog && <div className="creation-dialog-backdrop">
        <section className="creation-dialog" role="dialog" aria-modal="true" aria-label={creationDialog === 'board' ? t.newBoard : t.newThread} ref={creationDialogElement}>
          {creationDialog === 'board' ? <form id="conversation-creation-form" className="creation-form" onSubmit={submitBoard}>
            <label className="field"><span>{t.boardName}</span><input aria-describedby="board-name-count" ref={(element) => { if (element) creationDialogFirstField.current = element }} value={boardName} onChange={(event) => { setBoardName(event.target.value); setBoardFormError('') }} disabled={!canCreate} /></label>
            <span id="board-name-count" className={`byte-count${characterCount(boardName.trim()) > BOARD_NAME_LIMIT ? ' is-over-limit' : ''}`} role="status">{characterCounter(boardName.trim(), BOARD_NAME_LIMIT, language)}</span>
            <label className="field"><span>{t.description}</span><textarea value={boardDescription} onChange={(event) => setBoardDescription(event.target.value)} disabled={!canCreate} rows={3} /></label>
            {boardFormError && <p className="form-error" role="alert">{boardFormError}</p>}
          </form> : <form id="conversation-creation-form" className="creation-form" onSubmit={submitThread}>
            <label className="field"><span>{t.parentBoard}</span><select ref={(element) => { if (element) creationDialogFirstField.current = element }} value={threadBoardId} onChange={(event) => { setThreadBoardId(event.target.value); setThreadFormError('') }} disabled={!canCreate || boards.length === 0}>
              <option value="">{t.chooseBoard}</option>{allowedThreadBoards.map((board) => <option key={board.id} value={board.id}>{board.name}</option>)}
            </select></label>
            <label className="field"><span>{t.threadName}</span><input aria-describedby="thread-name-count" value={threadName} onChange={(event) => { setThreadName(event.target.value); setThreadFormError('') }} disabled={!canCreate} /></label>
            <span id="thread-name-count" className={`byte-count${characterCount(threadName.trim()) > THREAD_NAME_LIMIT ? ' is-over-limit' : ''}`} role="status">{characterCounter(threadName.trim(), THREAD_NAME_LIMIT, language)}</span>
            <label className="field"><span>{t.description}</span><textarea value={threadDescription} onChange={(event) => { setThreadDescription(event.target.value); setThreadFormError('') }} disabled={!canCreate} rows={3} /></label>
            {threadFormError && <p className="form-error" role="alert">{threadFormError}</p>}
          </form>}
          <footer className="creation-dialog-footer"><button className="button button-primary" type="submit" form="conversation-creation-form" disabled={!canCreate || (creationDialog === 'thread' && boards.length === 0)}>{creationBusy ? t.loading : t.create}</button><button className="button button-secondary" type="button" onClick={() => closeCreationDialog()}>{t.closeCreation}</button></footer>
        </section>
      </div>}
      {postHistory && <div className="history-backdrop" inert={pendingExternalUrl !== null || undefined} aria-hidden={pendingExternalUrl !== null || undefined}>
        <section className="history-drawer" role={pendingExternalUrl ? undefined : 'dialog'} aria-modal={pendingExternalUrl ? undefined : true} aria-labelledby="post-history-title" ref={historyDialog}>
          <header className="history-drawer-heading"><div><span className="workspace-kicker">No. {postHistory.no}</span><h2 id="post-history-title">{t.historyTitle}</h2></div><button className="button button-secondary" type="button" aria-label={t.closeHistory} ref={historyCloseButton} onClick={closePostHistory}>{t.closeHistory}</button></header>
          <div className="history-drawer-content" aria-live="polite">
            {postHistory.state === 'loading' && <p role="status">{t.historyLoading}</p>}
            {postHistory.state === 'failed' && <p className="workspace-error" role="alert">{t.historyFailed}</p>}
            {postHistory.state === 'ready' && !historyPost && <p className="workspace-error" role="alert">{t.historyFailed}</p>}
            {postHistory.state === 'ready' && historyPost && <>
              <section className="history-version current-version"><h3>{t.currentVersion} · revision {historyPost.revision}</h3><MarkdownBody language={language} onOpenLink={requestExternalOpen}>{historyPost.body}</MarkdownBody></section>
              {postHistory.items.length === 0 ? <p>{t.historyEmpty}</p> : <div className="history-version-list">{postHistory.items.map((item) => <article className="history-version" key={item.revision}>
                <h3>{t.beforeRevision(item.revision)}</h3><MarkdownBody language={language} onOpenLink={requestExternalOpen}>{item.body}</MarkdownBody>
                <dl><div><dt>{t.updatedBy}</dt><dd>{historyEditorLabel(item, language)}</dd></div><div><dt>{t.savedAt}</dt><dd><time dateTime={item.saved_at}>{localizedDate(item.saved_at, language)}</time></dd></div></dl>
              </article>)}</div>}
            </>}
          </div>
        </section>
      </div>}
      {deletedOpen && <div className="history-backdrop management-backdrop">
        <section className="history-drawer management-drawer" role={managementTarget || bulkManagementAction ? undefined : 'dialog'} aria-modal={managementTarget || bulkManagementAction ? false : true} aria-hidden={managementTarget || bulkManagementAction ? true : undefined} inert={!!managementTarget || !!bulkManagementAction} aria-labelledby="deleted-items-title" ref={deletedDialog}>
          <header className="history-drawer-heading"><div><span className="workspace-kicker">{t.deletedItems}</span><h2 id="deleted-items-title">{t.deletedItemsTitle}</h2></div><div className="workspace-actions deleted-heading-actions"><button className="button button-secondary" type="button" onClick={() => setDeletedSelectionMode((value) => !value)} aria-pressed={deletedSelectionMode}>{deletedSelectionMode ? t.finishSelecting : t.selectDeleted}</button><button className="button button-secondary" type="button" aria-label={t.closeDeletedItems} ref={deletedCloseButton} onClick={() => { setDeletedOpen(false); setSelectedDeletedKeys(new Set()); setDeletedSelectionMode(false) }}>{t.closeDeletedItems}</button></div></header>
          <div className="history-drawer-content" aria-live="polite">
            {deletedState === 'loading' && <p role="status">{t.deletedItemsLoading}</p>}
            {deletedState === 'failed' && <p className="workspace-error" role="alert">{t.deletedItemsFailed}</p>}
            {deletedState === 'ready' && deletedItems && deletedItems.boards.length === 0 && deletedItems.threads.length === 0 && <p>{t.noDeletedItems}</p>}
            {deletedState === 'ready' && deletedItems && <>
              {deletedSelectionMode && <section className="deleted-bulk-toolbar" aria-label={t.deletedItemsTitle}>
                <div className="workspace-actions deleted-selection-actions"><button className="button button-secondary" type="button" onClick={selectAllDeleted} disabled={managementBusy}>{t.selectAllDeleted}</button><button className="button button-secondary" type="button" onClick={() => setSelectedDeletedKeys(new Set())} disabled={!selectedDeletedKeys.size || managementBusy}>{t.clearDeletedSelection}</button></div>
                <span aria-live="polite">{t.selectedDeletedCount(selectedDeletedKeys.size)}</span>
                {selectedHasStaleRows && <p className="workspace-error" role="alert">{t.staleSelection}</p>}
                {selectedBlockedChild && <p>{t.parentMustBeSelected}</p>}
                <div className="workspace-actions deleted-bulk-actions"><button className="button button-secondary" type="button" onClick={() => openBulkManagement('restore')} disabled={!bulkRestoreAllowed || managementBusy || managementNeedsRefresh}>{t.bulkRestore}</button><button className="button button-danger" type="button" onClick={() => openBulkManagement('purge')} disabled={!bulkPurgeAllowed || managementBusy || managementNeedsRefresh}>{t.bulkPurge}</button></div>
              </section>}
              {deletedItems.boards.length > 0 && <section className="deleted-list-section" aria-labelledby="deleted-boards-title"><h3 id="deleted-boards-title">{t.deletedBoards}</h3><ul className="deleted-items-list">{deletedItems.boards.map((item) => {
                const target: ManagementTarget = { kind: 'board', id: item.id, name: item.name, action: 'restore' }
                const purgeTarget: ManagementTarget = { ...target, action: 'purge' }
                const key = deletedItemKey(target)
                return <li key={item.id}><div className="deleted-item-copy">{deletedSelectionMode && <label className="deleted-item-select"><input type="checkbox" aria-label={`${t.selectDeleted}: ${item.name}`} checked={selectedDeletedKeys.has(key)} onChange={(event) => toggleDeletedSelection(target, event.currentTarget.checked)} disabled={managementBusy || (item.can_restore === false && item.can_purge !== true)} /><span>{language === 'ja' ? '選択' : 'Select'}</span></label>}<strong>{item.name}</strong><time dateTime={item.deleted_at}>{t.deletedAt}: {localizedDate(item.deleted_at, language)}</time>{item.description && <p>{item.description}</p>}</div><div className="workspace-actions deleted-item-actions"><button className="button button-secondary" type="button" onClick={(event) => openManagement(target, event.currentTarget)} disabled={!canManage(target)}>{t.restore}</button><button className="button button-danger" type="button" onClick={(event) => openManagement(purgeTarget, event.currentTarget)} disabled={!canManage(purgeTarget)}>{t.permanentlyDelete}</button></div></li>
              })}</ul></section>}
              {deletedItems.threads.length > 0 && <section className="deleted-list-section" aria-labelledby="deleted-threads-title"><h3 id="deleted-threads-title">{t.deletedThreads}</h3><ul className="deleted-items-list">{deletedItems.threads.map((item) => {
                const parent = item.board_id ? boards.find((board) => board.id === item.board_id)?.name || null : null
                const target: ManagementTarget = { kind: 'thread', id: item.id, name: item.name, action: 'restore', boardName: parent }
                const purgeTarget: ManagementTarget = { ...target, action: 'purge' }
                const key = deletedItemKey(target)
                return <li key={item.id}><div className="deleted-item-copy">{deletedSelectionMode && <label className="deleted-item-select"><input type="checkbox" aria-label={`${t.selectDeleted}: ${item.name}`} checked={selectedDeletedKeys.has(key)} onChange={(event) => toggleDeletedSelection(target, event.currentTarget.checked)} disabled={managementBusy || (item.can_restore === false && item.can_purge !== true)} /><span>{language === 'ja' ? '選択' : 'Select'}</span></label>}<strong>{item.name}</strong>{parent && <span>{parent}</span>}{item.parent_deleted === true && <span>{language === 'ja' ? '親Boardも削除済み' : 'Parent Board is also deleted'}</span>}<time dateTime={item.deleted_at}>{t.deletedAt}: {localizedDate(item.deleted_at, language)}</time>{item.description && <p>{item.description}</p>}</div><div className="workspace-actions deleted-item-actions"><button className="button button-secondary" type="button" onClick={(event) => openManagement(target, event.currentTarget)} disabled={!canManage(target)}>{t.restore}</button><button className="button button-danger" type="button" onClick={(event) => openManagement(purgeTarget, event.currentTarget)} disabled={!canManage(purgeTarget)}>{t.permanentlyDelete}</button></div></li>
              })}</ul></section>}
            </>}
          </div>
        </section>
      </div>}
      {bulkManagementAction && <div className="history-backdrop management-confirm-backdrop">
        <section className="management-confirm-dialog" role="dialog" aria-modal="true" aria-labelledby="management-confirm-title" aria-describedby="management-impact" ref={managementDialog}>
          <h2 id="management-confirm-title">{bulkManagementAction === 'purge' ? t.confirmPurgeTitle : t.confirmBulkRestoreTitle}</h2>
          <p className="management-target-name">{t.selectedDeletedCount(selectedDeletedRows.length)}</p>
          <ul className="management-impact-list">{selectedDeletedRows.map((item) => <li key={deletedItemKey(item)}>{item.kind === 'board' ? t.boards : t.threadName}: {item.name}</li>)}</ul>
          <p id="management-impact">{bulkManagementAction === 'purge' ? t.bulkPurgeImpact : t.bulkRestoreImpact}</p>
          {bulkManagementAction === 'purge' && selectedDeletedRows.some((item) => item.kind === 'board') && <p>{t.purgeBoardImpact}</p>}
          {bulkManagementAction === 'purge' && selectedDeletedRows.some((item) => item.kind === 'thread') && <p>{t.purgeThreadImpact}</p>}
          {managementError && <p className="workspace-error" role="alert">{managementError}</p>}
          <div className="workspace-actions management-dialog-actions"><button className="button button-secondary" type="button" onClick={() => setBulkManagementAction(null)} disabled={managementBusy}>{t.cancelManagement}</button><button className={`button ${bulkManagementAction === 'purge' ? 'button-danger' : 'button-primary'}`} type="button" ref={managementConfirmButton} onClick={() => void manageBulk()} disabled={managementBusy || managementNeedsRefresh || (bulkManagementAction === 'purge' ? !bulkPurgeAllowed : !bulkRestoreAllowed)}>{managementBusy ? t.loading : bulkManagementAction === 'purge' ? t.bulkPurge : t.bulkRestore}</button></div>
        </section>
      </div>}
      {managementTarget && <div className="history-backdrop management-confirm-backdrop">
        <section className="management-confirm-dialog" role="dialog" aria-modal="true" aria-labelledby="management-confirm-title" aria-describedby="management-impact" ref={managementDialog}>
          <h2 id="management-confirm-title">{managementTarget.action === 'purge' ? t.confirmPurgeTitle : managementTarget.action === 'delete' ? t.confirmDeleteTitle : t.confirmRestoreTitle}</h2>
          <p className="management-target-name">{managementTarget.kind === 'board' ? `${t.boards}: ${managementTarget.name}` : managementTarget.kind === 'thread' ? `${t.threadName}: ${managementTarget.name}` : `${t.sendPost}: ${managementTarget.threadName} · No. ${managementTarget.no}`}</p>
          {managementTarget.kind === 'thread' && managementTarget.boardName && <p className="management-parent">{t.parentBoard}: {managementTarget.boardName}</p>}
          {managementTarget.kind === 'post' && <p className="management-parent">{t.threadName}: {managementTarget.threadName}</p>}
          <p id="management-impact">{managementTarget.action === 'purge' ? managementTarget.kind === 'board' ? t.purgeBoardImpact : t.purgeThreadImpact : managementTarget.kind === 'post' && managementTarget.action === 'delete' ? t.postDeleteImpact : managementTarget.kind === 'thread' && managementTarget.action === 'delete' ? t.threadDeleteImpact : managementTarget.kind === 'board' && managementTarget.action === 'delete' ? t.boardDeleteImpact : t.restoreImpact}</p>
          <div className="workspace-actions management-dialog-actions"><button className="button button-secondary" type="button" onClick={() => setManagementTarget(null)} disabled={managementBusy}>{t.cancelManagement}</button><button className={`button ${managementTarget.action === 'purge' ? 'button-danger' : 'button-primary'}`} type="button" ref={managementConfirmButton} onClick={() => void manageTarget()} disabled={managementBusy || !canManage(managementTarget)}>{managementBusy ? t.loading : managementTarget.action === 'purge' ? t.permanentlyDelete : t.confirmManagement}</button></div>
        </section>
      </div>}
      {pendingExternalUrl && <div className="external-link-backdrop">
        <section className="external-link-dialog" data-testid="external-link-confirmation" role="dialog" aria-modal="true" aria-labelledby="external-link-title" aria-describedby="external-link-explanation" ref={externalLinkDialog} onKeyDown={handleExternalLinkKeyDown} tabIndex={-1}>
          <h2 id="external-link-title">{t.externalLinkTitle}</h2>
          <p id="external-link-explanation">{t.externalLinkExplanation}</p>
          <code className="external-link-url" data-testid="external-link-url">{pendingExternalUrl}</code>
          {externalLinkBusy && <p className="external-link-status" role="status">{t.externalLinkOpening}</p>}
          {externalLinkError && <p className="workspace-error" data-testid="external-link-error" role="alert">{t.externalLinkFailed}</p>}
          <div className="workspace-actions external-link-actions">
            <button className="button button-secondary" data-testid="external-link-cancel" type="button" onClick={closeExternalLink} disabled={externalLinkBusy}>{t.externalLinkCancel}</button>
            <button className="button button-primary" data-testid="external-link-confirm" type="button" ref={externalLinkConfirmButton} onClick={() => void confirmExternalLink()} disabled={externalLinkBusy || busy || creationBusy || creationStatus.pending}>{externalLinkBusy ? t.externalLinkOpening : t.externalLinkOpen}</button>
          </div>
        </section>
      </div>}
    </section>
  )
}
