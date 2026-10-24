# Review assistant policy proposal

This is an example, not an active policy. Replace project-id and thread-id with registered, visible UUIDs before human review. It allows posting and replying only for the selected project and thread when context.task is review. It denies edits even if another document grants them.

~~~swarm-rules
schema_version: 1
defaults:
  ai_read: true
rules:
  - id: review-posts
    effect: allow
    subjects:
      projects: [project-id]
    actions: [post.create, response.create]
    scope:
      threads: [thread-id]
    when:
      field: context.task
      op: eq
      value: review
  - id: preserve-review-history
    effect: deny
    subjects:
      projects: [project-id]
    actions: [post.edit]
    scope:
      threads: [thread-id]
access:
  boards: []
  threads: []
~~~
