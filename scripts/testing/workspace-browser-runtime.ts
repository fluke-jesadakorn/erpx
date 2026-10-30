import{workspaceBrowserFixtureProfile}from './development-host-profile';
import { existsSync, readFileSync, realpathSync, writeFileSync } from 'node:fs';
import { dirname, isAbsolute, join } from 'node:path';
import { startWorkspaceGateway } from '../local/workspace-gateway-runtime';
import { hasPortListener } from '../local/port-listeners';

import { workspaceBrowserApps, workspaceHoldingProfile, workspaceBrowserOrigins, assertWorkspaceHoldingBinding } from "./development-host-profile";
import { workspaceBrowserBundler } from "./development-host-profile";
import { assertBrowserWorkspaceModules } from './browser-workspace-modules';

/** Executed only inside a disposable development-smoke source snapshot. */
export async function startWorkspaceBrowserRuntime() {
  const apps = workspaceBrowserApps(process.env.ERPX_BROWSER_WORKSPACE_APPS);
  const bundler = workspaceBrowserBundler(process.env.ERPX_BROWSER_WORKSPACE_BUNDLER);
  const consoleRetired=process.env.ERPX_BROWSER_WORKSPACE_CONSOLE_RETIRED==='1';
  const holdingProfile=workspaceHoldingProfile(process.env.ERPX_BROWSER_WORKSPACE_HOLDING,apps,consoleRetired,'engineering',3104);
  const fixtureProfile=workspaceBrowserFixtureProfile(process.env.ERPX_BROWSER_WORKSPACE_FIXTURE_PROFILE,apps,consoleRetired);
  const root = process.env.ERPX_REPOSITORY_ROOT, run = process.env.ERPX_BROWSER_SIMULATION_RUN_ID;
  if (process.version !== 'v26.11.1' || process.env.ERPX_BROWSER_EXECUTION_MODE !== 'development-smoke' || !apps || !root || !isAbsolute(root) || !run || !/^[a-f0-9]{12}$/.test(run) || !root.endsWith(`/.runtime/browser/${run}/app`) || process.env.ERPX_DATA_DIR !== dirname(root)) throw new Error('ISOLATED_WORKSPACE_BROWSER_RUNTIME_REQUIRED');
  await assertBrowserWorkspaceModules(root);
  const manifestPath = join(dirname(root), 'databases.json');
  if (process.env.ERPX_DATABASE_MANIFEST !== manifestPath || realpathSync(manifestPath) !== manifestPath) throw new Error('OWNED_WORKSPACE_BROWSER_MANIFEST_REQUIRED');
  const manifest = JSON.parse(readFileSync(manifestPath, 'utf8'));
  const bindings = ['CONTROL_DB', 'CONTROL_READ_DB', 'H001_DB', 'H001_READ_DB', 'H002_DB', 'H002_READ_DB', 'H003_DB', 'H003_READ_DB'];
  if (Object.keys(manifest.databases).length !== bindings.length || bindings.some(binding => {
    if (!manifest.databases[binding]) return true;
    const url = new URL(manifest.databases[binding]);
    return url.hostname !== '127.0.0.1' || url.port !== '5432' || url.username !== 'erpx_test_admin' || url.pathname !== `/erpx_browser_${run}_${binding.toLowerCase()}`;
  })) throw new Error('WORKSPACE_BROWSER_DATABASE_OWNERSHIP_REQUIRED');
  const state = JSON.parse(readFileSync(join(dirname(root), 'state.json'), 'utf8'));
  if((state.workspaceFixtureProfile??null)!==(fixtureProfile??null))throw new Error('WORKSPACE_FIXTURE_PROFILE_CHANGED');
  if(process.env.ERPX_HOLDING_ROUTING!==(holdingProfile?'enabled':undefined))throw new Error('HOLDING_BROWSER_ROUTING_CHANGED');
  assertWorkspaceHoldingBinding(holdingProfile,state.workspaceHoldingProfile);
  if(holdingProfile && state.profile!=='engineering')throw new Error('HOLDING_BROWSER_PROFILE_CHANGED');
  if(Boolean(state.workspaceConsoleRetired)!==consoleRetired)throw new Error('DEVELOPMENT_CONSOLE_PROFILE_CHANGED');
  if (state.run !== run || state.port !== 3104 || !state.ready || state.diagnostic || state.release || state.executionMode !== 'development-smoke' || JSON.stringify(state.workspaceApps) !== JSON.stringify(apps) || (state.workspaceBundler??'webpack')!==bundler) throw new Error('WORKSPACE_BROWSER_DEVELOPMENT_STATE_REQUIRED');
  if (hasPortListener(3104)) throw new Error('WORKSPACE_BROWSER_GATEWAY_PORT_OCCUPIED');
  const runtimePath=join(dirname(root),'workspace-runtime.json');
  let appPorts:Map<string,number>;
  if(existsSync(runtimePath)) {
    const prior=JSON.parse(readFileSync(runtimePath,'utf8'));
    if((prior.fixtureProfile??null)!==(fixtureProfile??null))throw new Error('WORKSPACE_FIXTURE_PROFILE_CHANGED');
    assertWorkspaceHoldingBinding(holdingProfile,prior.holdingProfile);
    if(Boolean(prior.consoleRetired)!==consoleRetired)throw new Error('DEVELOPMENT_CONSOLE_PROFILE_CHANGED');
    if(prior.schema!=='erpx-development-workspace-runtime/v1'||prior.acceptance!==false||prior.run!==run||prior.sourceRoot!==root||prior.sourceFingerprint!==state.sourceFingerprint||JSON.stringify(prior.apps)!==JSON.stringify(apps)||(prior.bundler??'webpack')!==bundler)throw new Error('WORKSPACE_BROWSER_RUNTIME_IDENTITY_CHANGED');
    appPorts=new Map(Object.entries(prior.appPorts) as [string,number][]);
    if(appPorts.size!==apps.length||apps.some(id=>!appPorts.has(id)))throw new Error("WORKSPACE_BROWSER_PORT_LEASE_CHANGED");
  }else{
    appPorts=new Map();let port=3250;
    for(const id of apps){while(port<=3299&&hasPortListener(port))port++;if(port>3299)throw new Error('WORKSPACE_BROWSER_APP_PORT_BUDGET_EXHAUSTED');appPorts.set(id,port++);}
  }
  const compatibilityOnly = apps.length === 1 && apps[0] === 'compat';
  const compatibilityOrigin='https://console-dev-erpx.jesadakorn.com';
  const gateway = await startWorkspaceGateway({ root, node: process.execPath, port: 3104, appPorts, preview: false,
    fallback: compatibilityOnly ? 'compat' : null,
    environment: { ...process.env, ...(compatibilityOnly ? {ERPX_PUBLIC_ORIGIN:compatibilityOrigin,BETTER_AUTH_URL:compatibilityOrigin} : { ERPX_PRODUCT_ROUTING: 'enabled', ERPX_PRODUCT_SSO: 'enabled', ERPX_PRODUCT_ENVIRONMENT: 'development' }),...(consoleRetired?{ERPX_CONSOLE_RETIRED:'enabled',ERPX_PUBLIC_ORIGIN:compatibilityOrigin}:{}) },
    settings: () => ({ version: 1, activeApps: apps.filter(id => id !== 'compat'), maxApps: Math.min(4, apps.length), idleMinutes: 10, bundlers: {}, defaultBundler: bundler }) });
  writeFileSync(runtimePath, JSON.stringify({ schema: 'erpx-development-workspace-runtime/v1', acceptance: false, run, pid: process.pid, gateway: 3104, sourceRoot: root, sourceFingerprint: state.sourceFingerprint, apps, bundler, consoleRetired, holdingProfile, fixtureProfile, compatibilityOnly, appPorts: Object.fromEntries(appPorts), origins: workspaceBrowserOrigins(apps,holdingProfile), leasePath: gateway.leasePath }, null, 2) + '\n', { mode: 0o600 });
  process.on('SIGTERM', () => { void gateway.close(); }); process.on('SIGINT', () => { void gateway.close(); });
  return gateway;
}
if (import.meta.main) await startWorkspaceBrowserRuntime();
