export type RuleDraft = {
  id: string
  effect: 'allow' | 'deny'
  action: string
  projectId?: string
  scope: 'global' | 'board' | 'thread'
  targetId?: string
  condition?: {
    field: string
    op: 'eq' | 'ne' | 'in' | 'contains' | 'lt' | 'lte' | 'gt' | 'gte' | 'exists'
    value?: unknown
  }
}

export const DEFAULT_RULES_TEMPLATE = `# Swarm rules
Describe the intent and scope of these rules here.

\`\`\`swarm-rules
schema_version: 1
defaults:
  ai_read: true
rules: []
access:
  boards: []
  threads: []
\`\`\`
`

type SourceLine = { text: string; start: number; end: number; newline: string }
type Fence = { openLine: number; closeLine: number; marker: '`' | '~'; length: number }

const MAX_SOURCE_LENGTH = 1_000_000
const MAX_VALUE_DEPTH = 16
const MAX_VALUE_NODES = 2048
const MAX_VALUE_LENGTH = 16_384
const CONDITION_OPERATORS = new Set(['eq', 'ne', 'in', 'contains', 'lt', 'lte', 'gt', 'gte', 'exists'])

function sourceLines(source: string): SourceLine[] {
  const lines: SourceLine[] = []
  let start = 0
  while (start < source.length) {
    let end = start
    while (end < source.length && source[end] !== '\n' && source[end] !== '\r') end += 1
    const newline = source.slice(end, end + (source.startsWith('\r\n', end) ? 2 : end < source.length ? 1 : 0))
    lines.push({ text: source.slice(start, end), start, end: end + newline.length, newline })
    start = end + newline.length
  }
  return lines
}

function commentStateAfter(line: string, initial: boolean): boolean {
  let inComment = initial
  for (let index = 0; index < line.length;) {
    if (inComment) {
      const end = line.indexOf('-->', index)
      if (end < 0) return true
      inComment = false
      index = end + 3
    } else {
      const start = line.indexOf('<!--', index)
      if (start < 0) return false
      inComment = true
      index = start + 4
    }
  }
  return inComment
}

function fenceOpen(line: string): { marker: '`' | '~'; length: number; info: string; indent: number } | null {
  const match = /^( *)(`{3,}|~{3,})(.*)$/.exec(line)
  if (!match || match[1].length > 3) return null
  if (match[2][0] === '`' && match[3].includes('`')) return null
  return { marker: match[2][0] as '`' | '~', length: match[2].length, info: match[3].trim(), indent: match[1].length }
}

function isFenceClose(line: string, marker: '`' | '~', length: number): boolean {
  const match = /^( {0,3})(`+|~+)[ \t]*$/.exec(line)
  return !!match && match[2][0] === marker && match[2].length >= length
}

function isRawHtmlBlockStart(line: string): boolean {
  return /^ {0,3}<(?:\/?[A-Za-z][A-Za-z0-9-]*(?:\s|\/?>)|![A-Z]|\?|!\[CDATA\[)/.test(line)
}

function locateRulesFence(lines: SourceLine[]): Fence {
  const found: Fence[] = []
  let active: { openLine: number; marker: '`' | '~'; length: number; isRules: boolean } | null = null
  let inHtmlComment = false
  for (let index = 0; index < lines.length; index += 1) {
    const text = lines[index].text
    if (active) {
      if (isFenceClose(text, active.marker, active.length)) {
        if (active.isRules) found.push({ openLine: active.openLine, closeLine: index, marker: active.marker, length: active.length })
        active = null
      }
      continue
    }
    if (inHtmlComment) {
      inHtmlComment = commentStateAfter(text, true)
      continue
    }
    if (!text.startsWith('<!--') && isRawHtmlBlockStart(text)) {
      throw new Error('RULES fences inside or alongside raw HTML blocks are unsupported; use a top-level swarm-rules fence.')
    }
    const candidate = /^( {1,})(`{3,}|~{3,})[ \t]*swarm-rules(?:[ \t].*)?$/.exec(text)
    if (candidate) throw new Error('Indented swarm-rules fences are unsupported.')
    const opening = fenceOpen(text)
    if (opening) {
      const startsRules = opening.info === 'swarm-rules' || opening.info.startsWith('swarm-rules ')
      if (startsRules && opening.info !== 'swarm-rules') throw new Error('Complex swarm-rules fence metadata is unsupported.')
      if (startsRules) active = { openLine: index, marker: opening.marker, length: opening.length, isRules: true }
      else active = { openLine: index, marker: opening.marker, length: opening.length, isRules: false }
      continue
    }
    inHtmlComment = commentStateAfter(text, false)
  }
  if (active?.isRules) throw new Error('The swarm-rules fence is not closed.')
  if (found.length !== 1) throw new Error(found.length === 0 ? 'Expected one top-level swarm-rules fence.' : 'Multiple top-level swarm-rules fences are unsupported.')
  return found[0]
}

function isBlankOrComment(line: string): boolean {
  return /^\s*(?:#.*)?$/.test(line)
}

function simpleTopLevelKey(line: string): string | null {
  const match = /^([A-Za-z_][A-Za-z0-9_-]*):(?:[ \t].*|$)/.exec(line)
  return match?.[1] ?? null
}

function stripYamlComment(line: string): string {
  let quote: '"' | "'" | null = null
  for (let index = 0; index < line.length; index += 1) {
    const char = line[index]
    if (quote === '"') {
      if (char === '\\') index += 1
      else if (char === '"') quote = null
      continue
    }
    if (quote === "'") {
      if (char === "'" && line[index + 1] === "'") index += 1
      else if (char === "'") quote = null
      continue
    }
    if (char === '"' || char === "'") quote = char
    else if (char === '#' && (index === 0 || /\s/.test(line[index - 1]))) return line.slice(0, index)
  }
  return line
}

function hasYamlAliasOrTag(line: string): boolean {
  const content = stripYamlComment(line)
  let quote: '"' | "'" | null = null
  for (let index = 0; index < content.length; index += 1) {
    const char = content[index]
    if (quote === '"') {
      if (char === '\\') index += 1
      else if (char === '"') quote = null
      continue
    }
    if (quote === "'") {
      if (char === "'" && content[index + 1] === "'") index += 1
      else if (char === "'") quote = null
      continue
    }
    if (char === '"' || char === "'") { quote = char; continue }
    const previous = index === 0 ? ' ' : content[index - 1]
    if (/[\s:[,{?-]/.test(previous) && (char === '&' || char === '*' || char === '!') && /[A-Za-z0-9_!<]/.test(content[index + 1] || '')) return true
  }
  return false
}

function appendPosition(lines: SourceLine[], fence: Fence): { offset: number; newline: string; emptyListToken?: { start: number; end: number } } {
  const blockStart = fence.openLine + 1
  const blockEnd = fence.closeLine
  const ruleIndexes: number[] = []
  let ruleLine = -1
  for (let index = blockStart; index < blockEnd; index += 1) {
    const key = simpleTopLevelKey(lines[index].text)
    if (key === 'rules') { ruleIndexes.push(index); ruleLine = index }
  }
  if (ruleIndexes.length !== 1) throw new Error(ruleIndexes.length ? 'Duplicate top-level rules keys are unsupported.' : 'A top-level rules key is required.')

  const line = lines[ruleLine].text
  const inline = /^rules([ \t]*):[ \t]*(\[\])[ \t]*(?:#.*)?$/.exec(line)
  const header = /^rules([ \t]*):(?:[ \t]*(?:#.*)?)$/.exec(line)
  if (!inline && !header) throw new Error('Only rules: [] or a normal block list can be extended.')
  if (inline) {
    const tokenStart = line.indexOf('[]')
    const nextKey = (() => {
      for (let index = ruleLine + 1; index < blockEnd; index += 1) {
        if (simpleTopLevelKey(lines[index].text)) return index
        if (!isBlankOrComment(lines[index].text) && !/^\s/.test(lines[index].text)) throw new Error('Unsupported top-level YAML wrapper.')
      }
      return blockEnd
    })()
    return { offset: lines[nextKey]?.start ?? lines[blockEnd].start, newline: lines[ruleLine].newline || lines[ruleLine - 1]?.newline || '\n', emptyListToken: { start: lines[ruleLine].start + tokenStart, end: lines[ruleLine].start + tokenStart + 2 } }
  }

  let boundary = blockEnd
  for (let index = ruleLine + 1; index < blockEnd; index += 1) {
    const text = lines[index].text
    if (isBlankOrComment(text)) continue
    if (/^[A-Za-z_][A-Za-z0-9_-]*:/.test(text)) { boundary = index; break }
    if (/^\s/.test(text)) continue
    throw new Error('Unsupported top-level YAML wrapper after rules.')
  }

  let hasItem = false
  for (let index = ruleLine + 1; index < boundary; index += 1) {
    const text = lines[index].text
    if (isBlankOrComment(text)) continue
    if (text.includes('\t')) throw new Error('Tabs in the rules block are unsupported.')
    const indent = /^ */.exec(text)?.[0].length ?? 0
    if (indent === 0) throw new Error('Unsupported top-level YAML wrapper inside rules.')
    const trimmed = text.slice(indent)
    if (indent === 2 && /^-(?:\s|$)/.test(trimmed)) hasItem = true
    else if (indent < 4 || indent === 3) throw new Error('Only a normal two-space rules block list is supported.')
    else if (!hasItem) throw new Error('A rules block must start with a normal list item.')
    if (hasYamlAliasOrTag(text)) throw new Error('YAML aliases and tags in rules are unsupported.')
  }

  let insertionLine = boundary
  while (insertionLine > ruleLine + 1 && isBlankOrComment(lines[insertionLine - 1].text)) insertionLine -= 1
  const offset = lines[insertionLine]?.start ?? lines[blockEnd].start
  const newline = lines[ruleLine].newline || lines[ruleLine - 1]?.newline || '\n'
  return { offset, newline }
}

function yamlString(value: unknown, label: string): string {
  if (typeof value !== 'string' || value.trim().length === 0 || value.length > 4096) throw new Error(`${label} must be a non-empty string.`)
  return JSON.stringify(value)
}

function yamlJsonValue(value: unknown): string {
  let nodes = 0
  const visit = (candidate: unknown, depth: number, seen: Set<object>): void => {
    nodes += 1
    if (nodes > MAX_VALUE_NODES || depth > MAX_VALUE_DEPTH) throw new Error('Condition value is too complex.')
    if (candidate === null || typeof candidate === 'string' || typeof candidate === 'boolean') return
    if (typeof candidate === 'number') {
      if (!Number.isFinite(candidate)) throw new Error('Condition numbers must be finite.')
      return
    }
    if (typeof candidate !== 'object') throw new Error('Condition value must be JSON-compatible.')
    if (seen.has(candidate)) throw new Error('Condition value cannot be cyclic.')
    const prototype = Object.getPrototypeOf(candidate)
    if (!Array.isArray(candidate) && prototype !== Object.prototype && prototype !== null) throw new Error('Condition value must use plain JSON data.')
    seen.add(candidate)
    if (Array.isArray(candidate)) candidate.forEach((item) => visit(item, depth + 1, seen))
    else {
      for (const key of Reflect.ownKeys(candidate)) {
        if (typeof key !== 'string') throw new Error('Condition objects cannot have symbol keys.')
        const descriptor = Object.getOwnPropertyDescriptor(candidate, key)
        if (!descriptor || !('value' in descriptor)) throw new Error('Condition objects cannot have getters or setters.')
        visit(descriptor.value, depth + 1, seen)
      }
    }
    seen.delete(candidate)
  }
  visit(value, 0, new Set())
  const serialized = JSON.stringify(value)
  if (serialized === undefined || serialized.length > MAX_VALUE_LENGTH) throw new Error('Condition value is not supported or is too large.')
  return serialized
}

function renderRule(draft: RuleDraft, newline: string): string {
  if (!draft || typeof draft !== 'object') throw new Error('Rule draft is required.')
  if (draft.effect !== 'allow' && draft.effect !== 'deny') throw new Error('Unsupported rule effect.')
  if (draft.scope !== 'global' && draft.scope !== 'board' && draft.scope !== 'thread') throw new Error('Unsupported rule scope.')
  const id = yamlString(draft.id, 'Rule id')
  const action = yamlString(draft.action, 'Rule action')
  const lines = [`  - id: ${id}`, `    effect: ${draft.effect}`]
  if (draft.projectId !== undefined) lines.push(`    subjects:`, `      projects: [${yamlString(draft.projectId, 'Project id')}]`)
  lines.push(`    actions: [${action}]`)
  if (draft.scope !== 'global') {
    if (draft.targetId === undefined) throw new Error('Board and Thread rules require a target id.')
    const targetId = yamlString(draft.targetId, 'Rule target id')
    const collection = draft.scope === 'board' ? 'boards' : 'threads'
    lines.push(`    scope:`, `      ${collection}: [${targetId}]`)
  } else if (draft.targetId !== undefined) {
    throw new Error('Global rules cannot include a target id.')
  }
  if (draft.condition !== undefined) {
    const condition = draft.condition
    const field = yamlString(condition.field, 'Condition field')
    if (!CONDITION_OPERATORS.has(condition.op)) throw new Error('Unsupported condition operator.')
    lines.push('    when:', '      all:', `        - field: ${field}`, `          op: ${condition.op}`)
    if (Object.prototype.hasOwnProperty.call(condition, 'value')) lines.push(`          value: ${yamlJsonValue(condition.value)}`)
  }
  return lines.join(newline)
}

/** Add one builder-created rule while preserving all surrounding Markdown/YAML bytes. */
export function appendRuleDraft(source: string, draft: RuleDraft): string {
  if (typeof source !== 'string' || source.length > MAX_SOURCE_LENGTH) throw new Error('Rules source is missing or too large.')
  const lines = sourceLines(source)
  const fence = locateRulesFence(lines)
  const position = appendPosition(lines, fence)
  const newline = position.newline
  const inserted = renderRule(draft, newline) + newline
  const edits: Array<{ start: number; end: number; text: string }> = [{ start: position.offset, end: position.offset, text: inserted }]
  if (position.emptyListToken) edits.push({ ...position.emptyListToken, text: '' })
  let result = source
  for (const edit of edits.sort((left, right) => right.start - left.start)) result = result.slice(0, edit.start) + edit.text + result.slice(edit.end)
  // Keep generation bounded even when callers pass unusually large string fields.
  if (result.length > MAX_SOURCE_LENGTH + MAX_VALUE_LENGTH) throw new Error('Rules output is too large.')
  return result
}
