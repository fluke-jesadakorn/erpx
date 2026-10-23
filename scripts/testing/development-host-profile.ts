import { createHash } from 'node:crypto';
import { readFileSync, realpathSync, lstatSync } from 'node:fs';
import { join } from 'node:path';
import { appDefinition } from '@erpx/contracts/apps';


export function workspaceBrowserApps(raw: string | undefined) {
  if (raw === undefined) return null;
  const apps: unknown = JSON.parse(raw);
  if (!Array.isArray(apps) || !apps.length || apps.length > 22 || new Set(apps).size !== apps.length || apps.some(id => typeof id !== 'string')) throw new Error('EXPLICIT_WORKSPACE_BROWSER_APPS_REQUIRED');
  if (apps.includes('compat') && (apps.length !== 1 || apps[0] !== 'compat')) throw new Error('COMPATIBILITY_BROWSER_PROFILE_MUST_BE_EXPLICIT');
  for (const id of apps) if (id !== 'compat') appDefinition(id);
  return apps as string[];
}

export function workspaceBrowserBundler(raw: string | undefined) {
  if (raw !== undefined && raw !== 'webpack' && raw !== 'turbopack') throw new Error('EXPLICIT_WORKSPACE_BROWSER_BUNDLER_REQUIRED');
  return raw ?? 'webpack';
}
export type DevelopmentHostProfile = { apps: string[]; entryApp: string; bundler?: 'webpack' | 'turbopack' };
export function developmentHostProfile(value: DevelopmentHostProfile): DevelopmentHostProfile {
  const apps = workspaceBrowserApps(JSON.stringify(value.apps));
  if (!apps || !apps.includes(value.entryApp)) throw new Error('DEVELOPMENT_HOST_ENTRY_APP_REQUIRED');
  workspaceBrowserBundler(value.bundler);
  return { apps, entryApp: value.entryApp, ...(value.bundler ? { bundler: value.bundler } : {}) };
}
export function developmentHostOrigin(app: string) {
  return app === 'compat' ? 'https://console-dev-erpx.jesadakorn.com' : appDefinition(app).origin;
}

/** Fail closed before launching a browser capable of reaching these host names. */
export function readDevelopmentHostLease(root: string, environment: NodeJS.ProcessEnv) {
  const path = environment.ERPX_DEVELOPMENT_HOST_LEASE;
  if (!path) return null;
  const run = environment.ERPX_DEMO_RUN;
  if (environment.ERPX_BROWSER_EXECUTION_MODE !== 'development-smoke' || !run || !/^[a-f0-9]{12}$/.test(run) || path !== join(root, '.runtime/browser', run, 'workspace-edge.json')) throw new Error('DEVELOPMENT_HOST_LEASE_MODE_REQUIRED');
  const directory = join(root, '.runtime/browser', run);
  function readPrivate(file: string) {
    const stat = lstatSync(file);
    if (realpathSync(file) !== file || !stat.isFile() || (stat.mode & 0o077) !== 0) throw new Error('PRIVATE_DEVELOPMENT_HOST_BINDING_REQUIRED');
    return readFileSync(file);
  }
  const lease = JSON.parse(readPrivate(path).toString()), state = JSON.parse(readPrivate(join(directory, 'state.json')).toString());
  if (lease.schema !== 'erpx-development-host-edge/v1' || lease.acceptance !== false || lease.run !== run || lease.port !== 3102 || lease.upstream !== 3104 || !/^[a-f0-9]{64}$/.test(lease.spki) || state.run !== run || state.executionMode !== 'development-smoke' || !state.ready || state.release || state.diagnostic || state.port !== 3104 || state.sourceFingerprint !== lease.sourceFingerprint) throw new Error('ISOLATED_DEVELOPMENT_HOST_LEASE_REQUIRED');
  const apps = workspaceBrowserApps(JSON.stringify(state.workspaceApps));
  if (!apps || JSON.stringify(lease.apps) !== JSON.stringify(apps) || JSON.stringify([...lease.origins].sort()) !== JSON.stringify(apps.map(developmentHostOrigin).sort()) || !lease.origins.includes(environment.ERPX_E2E_BASE_URL)) throw new Error('DEVELOPMENT_HOST_ORIGIN_BINDING_REQUIRED');
  if (lease.runtime.path !== join(directory, 'workspace-runtime.json') || !lease.certificate.path.startsWith(join(directory, 'workspace-tls-')) || !lease.certificate.path.endsWith('/certificate.pem')) throw new Error('DEVELOPMENT_HOST_RESOURCE_BINDING_REQUIRED');
  for (const binding of [lease.runtime, lease.certificate]) if (createHash('sha256').update(readPrivate(binding.path)).digest('hex') !== binding.sha256) throw new Error('DEVELOPMENT_HOST_BINDING_CHANGED');
  const runtime = JSON.parse(readPrivate(lease.runtime.path).toString());
  if (runtime.pid !== state.pid || runtime.run !== run || runtime.sourceRoot !== join(directory, 'app') || runtime.sourceFingerprint !== state.sourceFingerprint || JSON.stringify(runtime.apps) !== JSON.stringify(apps) || (runtime.bundler??'webpack')!==(state.workspaceBundler??'webpack')) throw new Error('DEVELOPMENT_HOST_RUNTIME_CHANGED');
  for (const pid of [lease.pid, runtime.pid]) { if (!Number.isSafeInteger(pid) || pid < 1) throw new Error('DEVELOPMENT_HOST_PROCESS_REQUIRED'); process.kill(pid, 0); }
  return lease as { pid: number; spki: string; origins: string[]; apps: string[] };
}
