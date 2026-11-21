import { mkdir, open, readFile, rm, symlink, writeFile } from 'node:fs/promises';
import { dirname, join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { appDefinition } from '@erpx/contracts/apps';
import { pinnedNode } from '../local/app-task';
const root = resolve(import.meta.dirname, '../..');
const id = process.argv.find(arg => arg.startsWith('--app='))?.slice(6);
if (!id) throw new Error('EXPLICIT_TYPECHECK_APP_REQUIRED');
const app = appDefinition(id), directory = join(root, app.directory), cache = join(directory, '.cache');
await mkdir(cache, { recursive: true });
const lock = join(cache, 'typecheck.lock');
let handle;
try { handle = await open(lock, 'wx', 0o600); }
catch (error) {
  if ((error as NodeJS.ErrnoException).code !== 'EEXIST') throw error;
  const owner = JSON.parse(await readFile(lock, 'utf8'));
  if (owner.root !== root || owner.app !== id) throw new Error('TYPECHECK_LOCK_OWNERSHIP_CHANGED');
  try { process.kill(owner.pid, 0); throw new Error('APP_TYPECHECK_ALREADY_RUNNING'); }
  catch (error) { if ((error as NodeJS.ErrnoException).code !== 'ESRCH') throw error; }
  await rm(lock); handle = await open(lock, 'wx', 0o600);
}
await handle.writeFile(JSON.stringify({ root, app: id, pid: process.pid }) + '\n'); await handle.close();
let child: ReturnType<typeof Bun.spawn> | undefined;
let interrupted = false;
const stop = () => { interrupted = true; child?.kill('SIGTERM'); }; process.once('SIGINT', stop); process.once('SIGTERM', stop);
try {
  // Next typegen rewrites next-env and tsconfig. Give it a private project so
  // concurrent development retains its own route types and output directory.
  const project = join(cache, 'typecheck-project');
  await rm(project, { recursive: true, force: true }); await mkdir(project, { recursive: true });
  await symlink(join(directory, 'src'), join(project, 'src'), 'dir');
  await writeFile(join(project, 'package.json'), JSON.stringify({ private: true, type: 'module' }) + '\n');
  await writeFile(join(project, 'next.config.mjs'), `import configuration from ${JSON.stringify(pathToFileURL(join(directory, 'next.config.mjs')).href)};\nexport default async (phase, context) => { const config = typeof configuration === 'function' ? await configuration(phase, context) : await configuration; return {...config,distDir:'.next',typescript:{...config.typescript,tsconfigPath:'tsconfig.json'}}; };\n`);
  await writeFile(join(project, 'tsconfig.json'), JSON.stringify({ extends: join(directory, 'tsconfig.json'), compilerOptions: { incremental: true, tsBuildInfoFile: join(cache, 'typecheck.tsbuildinfo') }, include: ['next-env.d.ts', join(directory,'src/**/*.ts'), join(directory,'src/**/*.tsx'), '.next/types/**/*.ts'], exclude: ['node_modules'] }, null, 2) + '\n');
  const node = pinnedNode(), environment = { ...process.env, ERPX_REPOSITORY_ROOT: root, ERPX_BUILD_DIR: '.next', NODE_OPTIONS: '--max-old-space-size=8192', PATH: dirname(node) + ':' + process.env.PATH };
  for (const args of [[node, join(root, 'node_modules/next/dist/bin/next'), 'typegen', project], [node, join(root, 'node_modules/@typescript/native/bin/tsc'), '--noEmit', '--project', join(project, 'tsconfig.json')]]) {
    if (interrupted) throw new Error('APP_TYPECHECK_INTERRUPTED');
    child = Bun.spawn(args, { cwd: root, env: environment, stdin: 'ignore', stdout: 'inherit', stderr: 'inherit' });
    if (await child.exited !== 0 || interrupted) throw new Error('APP_ROUTE_TYPECHECK_FAILED:' + id);
  }
} finally { process.off('SIGINT', stop); process.off('SIGTERM', stop); await rm(lock, { force: true }); }
