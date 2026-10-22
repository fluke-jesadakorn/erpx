import { createHash } from 'node:crypto';
import { mkdirSync, readFileSync, readdirSync, renameSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { repositoryRoot as root } from './catalogue';

/** Select entry files; TypeScript still follows every transitive type dependency. */
export function selectedTypecheckInputs(files: readonly string[]) {
  const selected = [...new Set(files)].sort();
  if (!selected.length || selected.some(file => !/^tests\/(?:unit|integration|e2e|journeys|uat|provider)\//.test(file) || file.split('/').includes('..')))
    throw new Error('EXPLICIT_TEST_TYPECHECK_FILES_REQUIRED');
  const declarations: string[] = [];
  function visit(directory: string) {
    for (const entry of readdirSync(resolve(root, directory), { withFileTypes: true })) {
      const path = `${directory}/${entry.name}`;
      if (entry.isDirectory() && !['node_modules','.cache','.next','.build'].includes(entry.name)) visit(path);
      else if (entry.isFile() && entry.name.endsWith('.d.ts')) declarations.push(path);
    }
  }
  visit('tests');
  return [...new Set([...selected, ...declarations])].sort();
}
export function selectedTypecheckConfig(files: readonly string[]) {
  const inputs = selectedTypecheckInputs(files);
  const hash = createHash('sha256').update(JSON.stringify(inputs));
  for (const path of ['tests/config/tsconfig.json', 'tsconfig.base.json']) hash.update(readFileSync(resolve(root, path)));
  const directory = resolve(root, '.scratch/dev/test-suites/typecheck'); mkdirSync(directory, { recursive: true, mode: 0o700 });
  const path = resolve(directory, hash.digest('hex') + '.json');
  const contents = JSON.stringify({ extends: resolve(root, 'tests/config/tsconfig.json'), compilerOptions: { noEmit: true, incremental: false }, files: inputs.map(file => resolve(root, file)), include: [], exclude: [] }, null, 2) + '\n';
  const temporary = `${path}.${process.pid}.tmp`; writeFileSync(temporary, contents, { mode: 0o600 }); renameSync(temporary, path);
  return path;
}
