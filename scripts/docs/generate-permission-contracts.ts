import { existsSync, readFileSync, renameSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
const root = resolve(import.meta.dir, '../..');
const args = process.argv.slice(2);
if (args.length > 1 || args.some(arg => arg !== '--check')) throw new Error('PERMISSION_CONTRACT_ARGUMENTS_INVALID');
const result = Bun.spawnSync([process.env.ERPX_CARGO_BIN ?? 'cargo', 'run', '--quiet', '--locked', '--package', 'erpx-server-identity-access', '--example', 'export-permission-contracts'], { cwd: root, stdout: 'pipe', stderr: 'pipe' });
if (result.exitCode !== 0) { process.stderr.write(result.stderr); throw new Error('PERMISSION_CONTRACT_GENERATION_FAILED'); }
const source = result.stdout.toString(), metadata: unknown = JSON.parse(source);
if (!Array.isArray(metadata) || !metadata.length || metadata.some(value => !value || typeof value.id !== 'string')) throw new Error('PERMISSION_CONTRACT_OUTPUT_INVALID');
const destination = resolve(root, 'packages/contracts/src/permission-catalog.generated.json');
if (args.includes('--check')) {
  if (!existsSync(destination) || readFileSync(destination, 'utf8') !== source) throw new Error('PERMISSION_CONTRACTS_STALE');
  console.log('Frontend permission metadata matches the canonical Rust catalog.');
} else {
  const temporary = `${destination}.${process.pid}.tmp`; writeFileSync(temporary, source); renameSync(temporary, destination);
  console.log('Generated frontend permission metadata from Rust.');
}
