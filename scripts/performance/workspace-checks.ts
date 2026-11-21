import { mkdir, writeFile } from 'node:fs/promises';
import { join, resolve, dirname } from 'node:path';
import { cpus, totalmem, platform, arch } from 'node:os';
import { loadCatalogue, repositoryRoot as root } from '../testing/catalogue';
import { loadWorkspaceGraph, selectWorkspaceSuites, workspaceFingerprint } from '../testing/workspace-graph';
import { refreshWorkspaceTaskInputs } from '../testing/workspace-tasks';
import { selectedTypecheckConfig } from '../testing/selected-typecheck';
import { pinnedNode } from '../local/app-task';

// Execute pure verification twice, rather than substituting an old unit pass.
// Integration/browser/provider lanes are excluded from this bounded benchmark.
const apps = ['finance', 'people', 'work'];
const node = pinnedNode();
if (Bun.version !== '1.4.3') throw new Error('BUN_1_4_3_REQUIRED');
refreshWorkspaceTaskInputs();
const directory = resolve(root, '.scratch/dev/performance/workspace-checks', new Date().toISOString().replace(/[:.]/g, '-'));
await mkdir(directory, { recursive: true, mode: 0o700 });
const graph = loadWorkspaceGraph(), catalogue = loadCatalogue();
const samples: { app: string; iteration: number; phase: string; durationMs: number; passed: boolean }[] = [];
const plans = apps.map(app => {
  const suites = selectWorkspaceSuites(graph, catalogue.suites, { apps: [app] }).suites.filter(suite => suite.lane === 'unit');
  const inputs = [...graph.apps.get(app)!, ...suites.flatMap(suite => suite.files)];
  return { app, suites, inputs, fingerprint: workspaceFingerprint(graph, inputs) };
});
for (const plan of plans) {
  for (const iteration of [0, 1]) {
    const commands = [
      { phase: 'lint-typecheck', args: [process.execPath, join(root, 'node_modules/turbo/bin/turbo'), 'run', 'lint', 'typecheck', `--filter=@erpx/app-${plan.app}`, '--cache=local:rw', '--concurrency=2', '--output-logs=errors-only'] },
      { phase: 'test-typecheck', args: [node, '--max-old-space-size=8192', join(root,'node_modules/@typescript/native/bin/tsc'), '--noEmit', '--project', selectedTypecheckConfig(plan.suites.flatMap(suite=>suite.files))] },
      { phase: 'unit', args: [process.execPath, 'scripts/testing/test.ts', 'unit', ...plan.suites.map(suite => `--suite=${suite.id}`)] },
    ];
    for (const command of commands) {
      console.log(`PURE_WORKSPACE_BENCHMARK:${plan.app}:${iteration}:${command.phase}:acceptance=false`);
      const started = performance.now(), log = join(directory, `${plan.app}-${iteration}-${command.phase}`);
      const child = Bun.spawn(command.args, { cwd: root, stdin: 'ignore', stdout: Bun.file(log + '.log'), stderr: Bun.file(log + '.errors.log'),
        env: { ...process.env, ERPX_NODE_BIN: node, PATH: dirname(process.execPath) + ':' + dirname(node) + ':' + process.env.PATH } });
      const passed = await child.exited === 0;
      samples.push({ app: plan.app, iteration, phase: command.phase, durationMs: performance.now() - started, passed });
      await writeFile(join(directory, 'samples.json'), JSON.stringify({ acceptance: false, samples }, null, 2) + '\n', { mode: 0o600 });
      if (!passed) throw new Error(`PURE_WORKSPACE_BENCHMARK_FAILED:${log}`);
    }
  }
}
const current = loadWorkspaceGraph();
if (plans.some(plan => workspaceFingerprint(current, plan.inputs) !== plan.fingerprint)) throw new Error('PURE_WORKSPACE_BENCHMARK_SOURCE_CHANGED');
const results = plans.map(plan => {
  const warmMs = samples.filter(sample => sample.app === plan.app && sample.iteration === 1).reduce((sum, sample) => sum + sample.durationMs, 0);
  return { app: plan.app, unitSuites: plan.suites.map(suite => suite.id), fingerprint: plan.fingerprint, warmMs, target: warmMs <= 120_000 };
});
const receipt = join(directory, 'receipt.json');
await writeFile(receipt, JSON.stringify({ scope: 'development-performance', acceptance: false, host: { platform: platform(), arch: arch(), cpus: cpus().length, memoryBytes: totalmem() }, samples, results }, null, 2) + '\n', { mode: 0o600 });
console.log(JSON.stringify({ receipt, results: results.map(({ app, warmMs, target }) => ({ app, warmMs, target })) }));
