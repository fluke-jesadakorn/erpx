import { existsSync, readFileSync, renameSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
const root = resolve(import.meta.dir, '../..');
const args = process.argv.slice(2);
if (args.length > 1 || args.some(value => value !== '--check')) throw new Error('FINANCE_JOURNAL_CREATE_CONTRACT_ARGUMENTS_INVALID');
const command = [process.env.ERPX_CARGO_BIN ?? 'cargo', 'run', '--quiet', '--locked', '--package', 'erpx-service', '--example', 'export-finance-journal-create-contracts', '--features', 'contracts'];
const source = Bun.spawnSync(command, { cwd: root, stdout: 'pipe', stderr: 'pipe' });
if (source.exitCode !== 0) { process.stderr.write(source.stderr); throw new Error('FINANCE_JOURNAL_CREATE_CONTRACT_GENERATION_FAILED'); }
const text = source.stdout.toString();
if (!text.startsWith('// Generated from the Rust Finance manual journal creation contracts.') || !text.includes('export type PreparedJournalResult')) throw new Error('FINANCE_JOURNAL_CREATE_CONTRACT_OUTPUT_INVALID');
const openapi = Bun.spawnSync([...command, '--', '--openapi'], { cwd: root, stdout: 'pipe', stderr: 'pipe' });
if (openapi.exitCode !== 0) { process.stderr.write(openapi.stderr); throw new Error('FINANCE_JOURNAL_CREATE_OPENAPI_GENERATION_FAILED'); }
const spec = openapi.stdout.toString(), parsed: unknown = JSON.parse(spec);
if (!parsed || typeof parsed !== 'object' || !('openapi' in parsed) || !('paths' in parsed)) throw new Error('FINANCE_JOURNAL_CREATE_OPENAPI_OUTPUT_INVALID');
for (const [relative, content] of [['packages/contracts/src/finance-journal-create.generated.ts', text], ['docs/contracts/next/finance-journal-create.openapi.generated.json', spec]]) {
  const destination = resolve(root, relative);
  if (args.includes('--check')) { if (!existsSync(destination) || readFileSync(destination, 'utf8') !== content) throw new Error(`FINANCE_JOURNAL_CREATE_CONTRACTS_STALE:${relative}`); }
  else { const temporary = `${destination}.${process.pid}.tmp`; writeFileSync(temporary, content); renameSync(temporary, destination); }
}
console.log(args.includes('--check') ? 'Rust Finance journal creation contracts match.' : 'Generated Rust Finance journal creation contracts.');
