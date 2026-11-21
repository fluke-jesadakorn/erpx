import { loadCatalogue, validateCatalogue, repositoryRoot } from './catalogue';

// Native units use the same exact-target dispatcher and case receipts as scoped CI.
// Database suites remain an explicit integration invocation with guarded fixtures.
if (Bun.version !== '1.4.3') throw new Error('Tests require Bun 1.4.3.');
const catalogue = loadCatalogue();
validateCatalogue(catalogue);
const selected = catalogue.suites.filter(suite => suite.framework === 'cargo' && suite.lane === 'unit');
if (!selected.length) throw new Error('NATIVE_RUST_UNIT_SELECTION_EMPTY');
const child = Bun.spawn([process.execPath, 'scripts/testing/test.ts', 'unit', ...selected.map(suite => `--suite=${suite.id}`)], { cwd: repositoryRoot, env: process.env, stdin: 'inherit', stdout: 'inherit', stderr: 'inherit' });
const interrupt = () => child.kill('SIGINT'), terminate = () => child.kill('SIGTERM');
process.on('SIGINT', interrupt); process.on('SIGTERM', terminate);
try { process.exitCode = await child.exited; }
finally { process.off('SIGINT', interrupt); process.off('SIGTERM', terminate); }
