import { existsSync, readFileSync, renameSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
const root = resolve(import.meta.dir, '../..');
const args = process.argv.slice(2);
if (args.length > 1 || args.some(value => value !== '--check')) throw new Error('FIELD_SERVICE_INITIAL_COUNT_CONTRACT_ARGUMENTS_INVALID');
const command = [process.env.ERPX_CARGO_BIN ?? 'cargo', 'run', '--quiet', '--locked', '--package', 'erpx-service', '--example', 'export-field-service-initial-count-contracts', '--features', 'contracts'];
const source = Bun.spawnSync(command, { cwd: root, stdout: 'pipe', stderr: 'pipe' });
if (source.exitCode !== 0) { process.stderr.write(source.stderr); throw new Error('FIELD_SERVICE_INITIAL_COUNT_CONTRACT_GENERATION_FAILED'); }
const text = source.stdout.toString();
if (!text.startsWith('// Generated from the Rust Service initial physical Count contracts.') || !text.includes('export type CountWorkspace')) throw new Error('FIELD_SERVICE_INITIAL_COUNT_CONTRACT_OUTPUT_INVALID');
const openapi = Bun.spawnSync([...command, '--', '--openapi'], { cwd: root, stdout: 'pipe', stderr: 'pipe' });
if (openapi.exitCode !== 0) { process.stderr.write(openapi.stderr); throw new Error('FIELD_SERVICE_INITIAL_COUNT_OPENAPI_GENERATION_FAILED'); }
const spec = openapi.stdout.toString(), parsed: unknown = JSON.parse(spec);
if (!parsed || typeof parsed !== 'object' || !('openapi' in parsed) || !('paths' in parsed)) throw new Error('FIELD_SERVICE_INITIAL_COUNT_OPENAPI_OUTPUT_INVALID');
for (const [relative, content] of [['packages/contracts/src/field-service-initial-count.generated.ts', text], ['docs/contracts/next/field-service-initial-count.openapi.generated.json', spec]]) {
  const destination = resolve(root, relative);
  if (args.includes('--check')) { if (!existsSync(destination) || readFileSync(destination, 'utf8') !== content) throw new Error(`FIELD_SERVICE_INITIAL_COUNT_CONTRACTS_STALE:${relative}`); }
  else { const temporary = `${destination}.${process.pid}.tmp`; writeFileSync(temporary, content); renameSync(temporary, destination); }
}
console.log(args.includes('--check') ? 'Rust Service initial physical Count contracts match.' : 'Generated Rust Service initial physical Count contracts.');
