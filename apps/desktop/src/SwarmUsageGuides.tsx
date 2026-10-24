type Language = 'ja' | 'en'

const projectCopy = {
  ja: {
    title: 'プロジェクトとRULES.mdの使い分け',
    intro: 'プロジェクト登録は、AIセッションがどの作業フォルダーに属するかを識別するための設定です。登録だけではAIの書き込み権限は増えません。',
    global: '共通ルール：全プロジェクトに適用',
    globalBody: 'Swarmのデータ保存先にあるRULES.mdが共通ルールです。「権限・ルール」の「全体ルール」で設定します。プロジェクトを限定しないルールを記述すれば、共通の許可・制限として利用できます。',
    local: '個別ルール：必要な場合だけ追加',
    localBody: '登録した正本フォルダーのRULES.mdは任意です。置かなくても共通ルールに従って利用できます。個別ルールはそのプロジェクトだけに適用されます。操作に対する明示的な拒否は、共通・個別のどちらに書かれていても許可より優先されます。',
    instructions: 'AIへの共通指示：毎回Swarmを使うための案内',
    instructionsBody: 'RULES.mdは操作権限を定めます。すべてのプロジェクトでSwarmを使いたい場合は、各AIクライアントのユーザー共通指示から、共有のSwarm CLI利用手順を参照させてください。',
    steps: '利用開始の手順',
    step1: '実際に作業する既存フォルダーを、このタブで登録します。個別のRULES.mdを用意する必要はありません。',
    step2: '「権限・ルール」で、必要な閲覧・投稿・返信・編集の範囲を共通ルールに設定します。プロジェクト固有の条件がある場合だけ個別ルールを追加します。',
    step3: 'AIにCLI実行ファイル、Swarmのデータ保存先、この一覧のproject IDを伝えます。セッションを実際の登録プロジェクトへ紐付け、並行するAIセッションごとに別のsession aliasを使います。',
    boundary: '新しい作業フォルダーを使う際も、登録とセッションの紐付けは必要です。共通ルールを設定するだけで、全AIクライアントが自動的にSwarmを使い始めるわけではありません。',
  },
  en: {
    title: 'How projects and RULES.md work together',
    intro: 'Project registration identifies the working folder for an AI session. Registration alone does not grant AI write permissions.',
    global: 'Global Rules: shared across projects',
    globalBody: 'RULES.md in the Swarm data directory is the global policy. Configure it under Permissions & rules → Global Rules. Rules without a project restriction can provide shared permissions and limits.',
    local: 'Project Rules: optional additions',
    localBody: 'RULES.md in a registered canonical folder is optional. Without it, the project uses the global policy. Local Rules apply only to that project. An explicit operation denial in either document takes precedence over grants.',
    instructions: 'Shared AI instructions: use Swarm in every project',
    instructionsBody: 'RULES.md defines operation permissions. To use Swarm across projects, have each AI client’s user-wide instructions refer to the shared Swarm CLI usage procedure.',
    steps: 'Getting started',
    step1: 'Register the existing folder where the work actually happens in this tab. A project-local RULES.md is not required.',
    step2: 'In Permissions & rules, configure the required reading, posting, replying and editing scope in Global Rules. Add Project Rules only when project-specific conditions are needed.',
    step3: 'Give the AI the CLI executable, Swarm data directory and project ID from this list. Bind the session to its actual registered project and use a different session alias for every parallel AI session.',
    boundary: 'A new working folder still needs registration and session binding. Global Rules alone do not automatically make every AI client start using Swarm.',
  },
}

export function ProjectUsageGuide({ language }: { language: Language }) {
  const t = projectCopy[language]
  return <details className="settings-usage-guide" data-testid="project-usage-guide">
    <summary>{t.title}</summary>
    <div className="settings-usage-guide-content">
      <p>{t.intro}</p>
      <dl>
        <div><dt>{t.global}</dt><dd><code>{'<data-directory>/RULES.md'}</code><p>{t.globalBody}</p></dd></div>
        <div><dt>{t.local}</dt><dd><code>{'<project-root>/RULES.md'}</code><p>{t.localBody}</p></dd></div>
        <div><dt>{t.instructions}</dt><dd>{t.instructionsBody}</dd></div>
      </dl>
      <h4>{t.steps}</h4>
      <ol><li>{t.step1}</li><li>{t.step2}</li><li>{t.step3}</li></ol>
      <p>{t.boundary}</p>
    </div>
  </details>
}

export function RulesAiGuide({ language }: { language: Language }) {
  const ja = language === 'ja'
  return <aside className="settings-ai-guide" aria-labelledby="rules-ai-guide-title" data-testid="rules-ai-guide">
    <h3 id="rules-ai-guide-title">{ja ? 'AIとの対話でRULES.mdの下書きを作成できます' : 'Draft RULES.md through a conversation with AI'}</h3>
    <p>{ja
      ? '普段お使いのAIクライアントに、対象プロジェクト、見せたいBoard・Thread、許可する閲覧・投稿・返信・編集と条件を、自然言語で伝えてください。既存ルールを保ったRULES.mdの下書きと、変更点の説明を作成してもらえます。'
      : 'Describe the project, visible Boards and Threads, permitted reading, posting, replying and editing, and any conditions to your usual AI client in natural language. Ask for a RULES.md draft that preserves existing rules and explains the changes.'}</p>
    <p>{ja
      ? '下書きの適用先（全体ルール／プロジェクトルール）と許可範囲を確認し、「ルール編集」の「テキスト」に貼り付けます。「下書きを検証」後、「保存内容を確認」から内容を確認して保存してください。保存には、その範囲のルールを編集する権限が必要です。'
      : 'Review the destination (Global Rules or Project Rules) and permission scope, then paste the draft into Rules editor → Text. Choose Validate draft, then Review save and confirm the contents before saving. Saving requires Rules editing permission for that scope.'}</p>
  </aside>
}
