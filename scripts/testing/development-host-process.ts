import { spawn } from 'node:child_process';
import { mkdir, open, readFile, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { developmentHostProfile, type DevelopmentHostProfile } from './development-host-profile';

/** The Node transport runs beside the disposable gateway; Bun only orchestrates. */
export async function startDevelopmentHostProcess(input: { root: string; run: string; output: string; node: string; sourceFingerprint: string; profile: DevelopmentHostProfile }) {
  const profile = developmentHostProfile(input.profile), directory = join(input.root, '.runtime/browser', input.run);
  const deadline = Date.now() + 30_000;
  while (true) {
    try {
      const runtime = JSON.parse(await readFile(join(directory, 'workspace-runtime.json'), 'utf8'));
      if (runtime.run !== input.run || runtime.sourceFingerprint !== input.sourceFingerprint || JSON.stringify(runtime.apps) !== JSON.stringify(profile.apps)) throw new Error('WORKSPACE_BROWSER_RUNTIME_CHANGED');
      process.kill(runtime.pid, 0); break;
    } catch (error) {
      if ((error as NodeJS.ErrnoException).code !== 'ENOENT' || Date.now() >= deadline) throw error;
      await new Promise(done => setTimeout(done, 100));
    }
  }
  const output = join(input.output, 'development-host'); await mkdir(output, { mode: 0o700 });
  const built = await Bun.build({ entrypoints: [join(input.root, 'scripts/testing/development-host-edge.ts')], outdir: output, target: 'node', format: 'esm', naming: 'edge.mjs', env: 'disable' });
  if (!built.success) throw new Error('DEVELOPMENT_HOST_EDGE_BUILD_FAILED');
  const descriptor = join(output, 'input.json');
  await writeFile(descriptor, JSON.stringify({ root: input.root, run: input.run, sourceFingerprint: input.sourceFingerprint, entryApp: profile.entryApp }), { mode: 0o600, flag: 'wx' });
  const log = await open(join(output, 'edge.log'), 'a', 0o600);
  const child = spawn(input.node, [join(output, 'edge.mjs'), descriptor], { cwd: input.root, stdio: ['ignore', log.fd, log.fd, 'ipc'] }); await log.close();
  let closing: Promise<void> | undefined;
  const close = () => closing ??= new Promise<void>((done, reject) => {
    if (child.exitCode !== null || child.signalCode !== null) { done(); return; }
    const timeout = setTimeout(() => reject(new Error('DEVELOPMENT_HOST_EDGE_DRAIN_TIMEOUT')), 10_000);
    child.once('exit', () => { clearTimeout(timeout); done(); }); child.kill('SIGTERM');
  });
  try {
    const leasePath = await new Promise<string>((done, reject) => {
      const timeout = setTimeout(() => reject(new Error('DEVELOPMENT_HOST_EDGE_START_TIMEOUT')), 120_000);
      const fail = () => { clearTimeout(timeout); reject(new Error('DEVELOPMENT_HOST_EDGE_EXITED')); };
      child.once('error', fail); child.once('exit', fail);
      child.on('message', (message: { kind?: string; path?: string }) => {
        if (message.kind !== 'ready' || message.path !== join(directory, 'workspace-edge.json')) return;
        clearTimeout(timeout); child.off('error', fail); child.off('exit', fail); done(message.path);
      });
    });
    const lease=JSON.parse(await readFile(leasePath,'utf8'));
    return { path: leasePath, certificatePath: lease.certificate.path as string, close };
  } catch (error) { await close(); throw error; }
}
