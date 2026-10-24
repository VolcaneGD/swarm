import { describe, expect, it } from 'vitest'
import { appendRuleDraft, DEFAULT_RULES_TEMPLATE, type RuleDraft } from './rulesDraft'

const simpleDraft: RuleDraft = {
  id: 'allow-review', effect: 'allow', action: 'response.create', projectId: 'project-a', scope: 'thread', targetId: 'thread-a',
}

describe('source-preserving RULES draft insertion', () => {
  it('provides a Markdown template with one structured rules block', () => {
    expect(DEFAULT_RULES_TEMPLATE).toContain('# Swarm rules')
    expect(DEFAULT_RULES_TEMPLATE.match(/^```swarm-rules$/gm)).toHaveLength(1)
    expect(DEFAULT_RULES_TEMPLATE).toContain('rules: []')
    expect(DEFAULT_RULES_TEMPLATE).toContain('access:')
  })

  it('replaces only the empty rules token and preserves its inline comment and surrounding Markdown bytes', () => {
    const source = '# Intro\r\nKeep this paragraph exactly.\r\n\r\n```swarm-rules\r\nschema_version: 1\r\nrules: [] # hand-written note\r\naccess:\r\n  boards: []\r\n```\r\n\r\nAfter the block.\r\n'
    const result = appendRuleDraft(source, simpleDraft)

    expect(result).toContain('rules:  # hand-written note\r\n  - id: "allow-review"\r\n')
    expect(result).toContain('      threads: ["thread-a"]\r\naccess:\r\n')
    expect(result.startsWith('# Intro\r\nKeep this paragraph exactly.\r\n\r\n```swarm-rules\r\n')).toBe(true)
    expect(result.endsWith('```\r\n\r\nAfter the block.\r\n')).toBe(true)
    expect(source).toContain('rules: [] # hand-written note')
  })

  it('appends after existing block-list entries but before trailing comments and the next top-level key', () => {
    const source = [
      'Explanation stays as written.',
      '~~~swarm-rules',
      'schema_version: 1',
      'rules:',
      '  - id: "keep-me"',
      '    effect: deny',
      '    actions: ["post.delete"]',
      '    when:',
      '      all:',
      '        - field: "context.purpose"',
      '          op: eq',
      '          value: "audit # this is data"',
      '# keep this access note at top level',
      'access:',
      '  boards: []',
      '~~~',
      '',
    ].join('\n')

    const result = appendRuleDraft(source, { id: 'deny-edit', effect: 'deny', action: 'post.edit', scope: 'global' })

    expect(result).toContain('          value: "audit # this is data"\n  - id: "deny-edit"\n    effect: deny\n    actions: ["post.edit"]\n# keep this access note at top level\naccess:')
    expect(result).toContain('~~~swarm-rules')
    expect(result).not.toContain('```swarm-rules')
    expect(result).toMatch(/access:\n  boards: \[\]\n~~~\n$/)
  })

  it('quotes scalar fields as JSON-compatible YAML and serializes a bounded condition value', () => {
    const result = appendRuleDraft(DEFAULT_RULES_TEMPLATE, {
      ...simpleDraft,
      id: 'line\n"id',
      action: 'post.edit',
      condition: { field: 'context.note', op: 'in', value: ['quote " safely', 3, true, null] },
    })

    expect(result).toContain('  - id: "line\\n\\"id"')
    expect(result).toContain('    when:\n      all:\n        - field: "context.note"\n          op: in\n          value: ["quote \\\" safely",3,true,null]')
  })

  it('handles a tilde fence and leaves an unrelated fenced code example byte-identical', () => {
    const source = '```yaml\nrules: []\n```\n\n~~~swarm-rules\nschema_version: 1\nrules: []\n~~~\n'
    const result = appendRuleDraft(source, { id: 'a', effect: 'allow', action: 'board.list', scope: 'global' })
    expect(result.startsWith('```yaml\nrules: []\n```\n\n~~~swarm-rules\n')).toBe(true)
    expect(result).toContain('rules: \n  - id: "a"\n')
    expect(result.endsWith('~~~\n')).toBe(true)
  })

  it('ignores commented and nested fence examples when locating the single actual block', () => {
    const source = [
      '<!--',
      '```swarm-rules',
      'rules: []',
      '```',
      '-->',
      '```markdown',
      '~~~swarm-rules',
      'rules: []',
      '~~~',
      '```',
      '```swarm-rules',
      'rules: []',
      '```',
      '',
    ].join('\n')

    const result = appendRuleDraft(source, { id: 'actual', effect: 'allow', action: 'board.list', scope: 'global' })

    expect(result).toContain('<!--\n```swarm-rules\nrules: []\n```\n-->')
    expect(result).toContain('```markdown\n~~~swarm-rules\nrules: []\n~~~\n```')
    expect(result.match(/id: "actual"/g)).toHaveLength(1)
    expect(result.endsWith('rules: \n  - id: "actual"\n    effect: allow\n    actions: ["board.list"]\n```\n')).toBe(true)
  })

  it.each([
    ['non-empty flow list', 'rules: [ { id: old } ]'],
    ['alias value', 'rules: *shared'],
    ['anchor value', 'rules: &shared'],
    ['tag value', 'rules: !custom []'],
    ['non-list mapping value', 'rules: { id: old }'],
  ])('refuses a %s without modifying the caller source', (_name, line) => {
    const source = `Intro\n\`\`\`swarm-rules\nschema_version: 1\n${line}\naccess: {}\n\`\`\`\n`
    expect(() => appendRuleDraft(source, simpleDraft)).toThrow()
    expect(source).toContain(line)
  })

  it('refuses missing, multiple, indented, commented-only, nested-in-other-fence, and unclosed rules fences', () => {
    const cases = [
      'No structured block here.\n',
      '```swarm-rules\nrules: []\n```\n\n~~~swarm-rules\nrules: []\n~~~\n',
      '  ```swarm-rules\nrules: []\n  ```\n',
      '<!--\n```swarm-rules\nrules: []\n```\n-->\n',
      '```markdown\n```swarm-rules\nrules: []\n```\n```\n',
      '```swarm-rules\nrules: []\n',
      '<div>\n~~~swarm-rules\nrules: []\n~~~\n</div>\n\n~~~swarm-rules\nrules: []\n~~~\n',
      '<script>\nconst example = true\n\n~~~swarm-rules\nrules: []\n~~~\n</script>\n',
    ]
    for (const source of cases) expect(() => appendRuleDraft(source, simpleDraft)).toThrow()
  })

  it('rejects aliases and tags inside the existing block-list YAML', () => {
    for (const special of ['  - id: *rule-id', '  - id: rule\n    effect: !custom allow']) {
      const source = `\`\`\`swarm-rules\nrules:\n${special}\naccess: {}\n\`\`\`\n`
      expect(() => appendRuleDraft(source, simpleDraft)).toThrow(/aliases and tags/)
    }
  })

  it('validates draft scope, required target, condition operator, and JSON-safe values', () => {
    expect(() => appendRuleDraft(DEFAULT_RULES_TEMPLATE, { ...simpleDraft, scope: 'project' as never })).toThrow(/scope/)
    expect(() => appendRuleDraft(DEFAULT_RULES_TEMPLATE, { ...simpleDraft, scope: 'board', targetId: undefined })).toThrow(/target id/)
    expect(() => appendRuleDraft(DEFAULT_RULES_TEMPLATE, { ...simpleDraft, condition: { field: 'x', op: 'exec' as never } })).toThrow(/operator/)
    const cyclic: Record<string, unknown> = {}
    cyclic.self = cyclic
    expect(() => appendRuleDraft(DEFAULT_RULES_TEMPLATE, { ...simpleDraft, condition: { field: 'x', op: 'eq', value: cyclic } })).toThrow(/cyclic/)
    expect(() => appendRuleDraft(DEFAULT_RULES_TEMPLATE, { ...simpleDraft, condition: { field: 'x', op: 'eq', value: Number.NaN } })).toThrow(/finite/)
  })
})
