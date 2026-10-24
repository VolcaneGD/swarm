// Controlled server-rendering comparison; never claims native WebView2 memory.
import { createRequire } from 'node:module'
import { readFile, writeFile, mkdir } from 'node:fs/promises'
import { resolve, dirname } from 'node:path'
import { fileURLToPath } from 'node:url'
import { spawnSync } from 'node:child_process'
import { cpus, totalmem } from 'node:os'

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..')
const [baselineArg, outputArg] = process.argv.slice(2)
if (!baselineArg || !outputArg) throw new Error('Expected baseline source and new output JSON')
const baseline = resolve(root, baselineArg), output = resolve(root, outputArg)
const sourceDir = resolve(root, 'apps/desktop/src')
const require = createRequire(resolve(root, 'apps/desktop/package.json'))
const { build } = require('esbuild')
await mkdir(dirname(output), { recursive: true })
const harness = `
import {createElement} from 'react';
import {renderToStaticMarkup} from 'react-dom/server';
import {createHash} from 'node:crypto';
import MarkdownBody from 'swarm-markdown-under-test';
const workload=process.argv[2];
const rows=Array.from({length:1000},(_,i)=>workload==='mixed'&&i%2===0
  ? '## Post '+i+'\\n\\n**Text** with [link](https://example.com) and \`code\`.'
  : workload==='long'&&i===999 ? 'D'.repeat(100000) : 'Sample Post '+i+' contains a short ordinary paragraph.');
global.gc?.();
const initial=process.memoryUsage();const start=performance.now();
const html=renderToStaticMarkup(createElement('section',null,rows.map((body,i)=>createElement(MarkdownBody,{key:i,children:body,language:'ja',onOpenLink:()=>{}}))));
const elapsed=performance.now()-start;const after=process.memoryUsage();
console.log(JSON.stringify({workload,rows:rows.length,elapsed_ms:elapsed,peak_rss_bytes:process.resourceUsage().maxRSS*1024,initial_rss_bytes:initial.rss,heap_used_after_bytes:after.heapUsed,html_bytes:Buffer.byteLength(html),html_sha256:createHash('sha256').update(html).digest('hex')}));
`
const observations = []
for (const [label, path] of [['baseline', baseline], ['candidate', resolve(sourceDir, 'MarkdownBody.tsx')]]) {
  const executable = resolve(dirname(output), `${label}-render.mjs`)
  const contents = await readFile(path, 'utf8')
  await build({
    stdin: { contents: harness, loader: 'tsx', resolveDir: sourceDir },
    outfile: executable, bundle: true, platform: 'node', format: 'esm', jsx: 'automatic',
    banner: { js: "import {createRequire as createNodeRequire} from 'node:module';const require=createNodeRequire(import.meta.url);" },
    define: { 'process.env.NODE_ENV': '"production"' }, loader: { '.css': 'empty' },
    plugins: [{ name: 'exact-markdown-source', setup(b) {
      b.onResolve({ filter: /^swarm-markdown-under-test$/ }, () => ({ path: 'MarkdownBody.tsx', namespace: 'benchmark' }))
      b.onLoad({ filter: /.*/, namespace: 'benchmark' }, () => ({ contents, loader: 'tsx', resolveDir: sourceDir }))
    } }],
  })
  for (const workload of ['literal', 'long', 'mixed']) for (let repetition = 1; repetition <= 2; repetition++) {
    const child = spawnSync(process.execPath, ['--expose-gc', executable, workload], { encoding: 'utf8', timeout: 30000, windowsHide: true })
    if (child.status !== 0) throw new Error(`Controlled renderer failed: ${label}/${workload}: ${child.stderr.slice(0, 1000)}`)
    observations.push({ implementation: label, repetition, ...JSON.parse(child.stdout) })
  }
}
for (const workload of ['literal', 'long', 'mixed']) {
  const hashes = new Set(observations.filter(o => o.workload === workload).map(o => o.html_sha256))
  if (hashes.size !== 1) throw new Error(`Rendering changed for ${workload}; do not adopt`)
}
const result = { scope: 'ISOLATED_NODE_SERVER_RENDERING_ONLY', node: process.version, cpu: cpus()[0].model,
  logical_processors: cpus().length, physical_ram_bytes: totalmem(), rows_per_render: 1000,
  native_gui_memory_acceptance: false, equivalent_html: true, observations }
await writeFile(output, `${JSON.stringify(result, null, 2)}\n`, { flag: 'wx', encoding: 'utf8' })
console.log(JSON.stringify(result))
