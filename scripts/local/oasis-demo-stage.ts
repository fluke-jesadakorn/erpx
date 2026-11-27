import { spawn, type ChildProcess } from 'node:child_process';
import { constants } from 'node:fs';
import { lstat, mkdir, open, readFile, realpath } from 'node:fs/promises';
import { isAbsolute, join, sep } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';
import { oasisDemoAccountHost, oasisDemoHolding } from '@erpx/contracts/holding-hosts';
import { oasisDemoRouting, startCompiledWorkspaceGateway } from './compiled-workspace-gateway';
import { hasPortListener } from './port-listeners';
import { oasisDemoApps, verifyWorkspaceRelease } from './workspace-release-manifest';

async function privatePath(path: string, directory = false) {
  const info = await lstat(path);
  if (!(directory ? info.isDirectory() : info.isFile()) || info.mode & 0o077 ||
    process.getuid !== undefined && info.uid !== process.getuid()) throw new Error('OASIS_PRIVATE_STATE_REQUIRED');
  return path;
}

if (import.meta.main || process.argv[1]?.endsWith('/stage/entry.mjs')) {
  if (process.version !== 'v26.11.1') throw new Error('PINNED_RELEASE_RUNTIME_REQUIRED');
  const args = process.argv.slice(2), releaseInput = args.find(arg => arg.startsWith('--release='))?.slice(10),
    stateInput = args.find(arg => arg.startsWith('--state='))?.slice(8);
  if (args.length !== 2 || !releaseInput || !isAbsolute(releaseInput) || !stateInput || !isAbsolute(stateInput)) throw new Error('EXPLICIT_OASIS_RELEASE_AND_STATE_REQUIRED');
  const release = await realpath(releaseInput), state = await realpath(await privatePath(stateInput, true));
  if (release === state || release.startsWith(state + sep) || state.startsWith(release + sep)) throw new Error('OASIS_STATE_MUST_BE_OUTSIDE_RELEASE');
  const manifest = await verifyWorkspaceRelease(release, {oasisDemo:true, frozen:true});
  const privateConfig = await privatePath(join(state, 'private-api.json'));
  const databases = await privatePath(join(state, 'databases.json'));
  const runtime = JSON.parse(await readFile(await privatePath(join(state, 'next-runtime.json')), 'utf8')) as {BETTER_AUTH_SECRET?:unknown};
  if (!runtime || Object.keys(runtime).length !== 1 || typeof runtime.BETTER_AUTH_SECRET !== 'string' ||
    runtime.BETTER_AUTH_SECRET.length < 32 || /^(generate-|local-development)/.test(runtime.BETTER_AUTH_SECRET)) throw new Error('OASIS_NEXT_SECRET_REQUIRED');
  for (const port of [oasisDemoRouting.gateway, oasisDemoRouting.api, ...Object.values(oasisDemoRouting.ports)]) {
    if (hasPortListener(port)) throw new Error(`OASIS_PORT_OCCUPIED:${port}`);
  }
  const logs = join(state, 'logs');
  await mkdir(logs, {mode:0o700, recursive:true});
  await privatePath(logs, true);
  const privateBaseUrl = `http://127.0.0.1:${oasisDemoRouting.api}`;
  const accountOrigin = `https://${oasisDemoAccountHost}`;
  const holdingOrigin = `https://${oasisDemoHolding.hostname}`;
  const ownerApi = JSON.stringify({privateBaseUrl, timeoutMs:5000});
  const account = JSON.stringify({privateBaseUrl, timeoutMs:5000, accountOrigin, holdingOrigins:[holdingOrigin], originProfile:'oasis-demo-staging'});
  const holding = JSON.stringify({privateBaseUrl, timeoutMs:5000, holdings:[{origin:holdingOrigin, holdingId:oasisDemoHolding.holdingId, holdingSlug:oasisDemoHolding.holdingSlug}]});
  const holdingBinding = JSON.stringify({privateBaseUrl, timeoutMs:5000, origin:holdingOrigin,
    host:oasisDemoHolding.hostname, holdingSlug:oasisDemoHolding.holdingSlug});
  const companyServices = JSON.stringify({privateBaseUrl, timeoutMs:5000, holdings:[{
    origin:holdingOrigin, host:oasisDemoHolding.hostname, holdingId:oasisDemoHolding.holdingId,
    holdingSlug:oasisDemoHolding.holdingSlug, companyId:'oasis-demo-company',
  }]});
  const env = {
    PATH:process.env.PATH, TZ:process.env.TZ, NODE_ENV:'production', NEXT_TELEMETRY_DISABLED:'1',
    ERPX_ENVIRONMENT:'staging', ERPX_PRODUCT_ENVIRONMENT:'staging', ERPX_OASIS_DEMO_STAGING:'1',
    ERPX_HOLDING_ROUTING:'enabled', ERPX_PRODUCT_ROUTING:'enabled', ERPX_PRODUCT_SSO:'enabled',
    ERPX_DATA_DIR:state, ERPX_DATABASE_MANIFEST:databases, ERPX_PROCESS_ROLE:'app',
    ERPX_APP_PG_POOL_MAX:'2', ERPX_CONTROL_PG_POOL_MAX:'2', BETTER_AUTH_SECRET:runtime.BETTER_AUTH_SECRET,
    ERPX_HOLDING_ADDRESS_API:privateBaseUrl,
    ERPX_NATIVE_RUNTIME_HOLDING_READ_CONFIG:holdingBinding,
    ERPX_NATIVE_ORGANIZATION_CONFIG:holding, ERPX_NATIVE_COMMERCIAL_OWNER_READ_CONFIG:holding,
    ERPX_NATIVE_PEOPLE_PAYROLL_CONFIG:companyServices,
    ERPX_NATIVE_COMMERCE_OWNER_WRITE_CONFIG:holdingBinding, ERPX_NATIVE_WORKFLOW_WAVE3_CONFIG:ownerApi,
    ERPX_NATIVE_FINANCE_COMPANY_MONEY_CONFIG:ownerApi, ERPX_NATIVE_FINANCE_LANE_2_CONFIG:companyServices,
    ERPX_NATIVE_BRANDING_CONFIG:holding,
    ERPX_NATIVE_IDENTITY_ACCESS_LANE_11_CONFIG:companyServices, ERPX_NATIVE_BRAND_LANE39_CONFIG:companyServices,
    ERPX_NATIVE_IDENTITY_CONTROL_READ_CONFIG:JSON.stringify({privateBaseUrl, timeoutMs:5000, origin:holdingOrigin,
      holdings:[{holdingSlug:oasisDemoHolding.holdingSlug}]}),
    ERPX_NATIVE_COMMERCE_LANE14_CONFIG:holdingBinding,
    ERPX_NATIVE_ACCOUNT_SESSION_CONFIG:account, ERPX_NATIVE_FIELD_SERVICE_WORKSPACE_CONFIG:holding,
    ERPX_NATIVE_CRM_CONFIG:companyServices,
    ERPX_NATIVE_CRM_PIPELINE_CONFIG:holding, ERPX_NATIVE_CRM_ACTIVITY_WORKSPACE_CONFIG:holding,
    ERPX_NATIVE_SALES_DOCUMENTS_CONFIG:holding, ERPX_NATIVE_COMMERCE_ORDER_CONFIG:holding,
  } satisfies NodeJS.ProcessEnv;
  const children = new Map<string, ChildProcess>();
  let shuttingDown = false, gateway: Awaited<ReturnType<typeof startCompiledWorkspaceGateway>> | undefined;
  const abort = new AbortController();
  let failed: string | undefined;
  async function start(name: string, executable: string, argv: string[], environment: NodeJS.ProcessEnv) {
    const path = join(logs, `${name}.log`);
    const log = await open(path, constants.O_CREAT | constants.O_APPEND | constants.O_WRONLY | constants.O_NOFOLLOW, 0o600);
    try {
      await privatePath(path);
      const child = spawn(executable, argv, {cwd:state, env:environment, stdio:['ignore',log.fd,log.fd]});
      children.set(name, child);
      const exited = () => {
        if (shuttingDown) return;
        failed = name;
        abort.abort();
      };
      child.once('exit', exited);
      child.once('error', exited);
    } finally { await log.close(); }
  }
  const stopped = new Promise<void>(done => abort.signal.addEventListener('abort', () => done(), {once:true}));
  for (const signal of ['SIGINT', 'SIGTERM'] as const) process.once(signal, () => abort.abort());
  try {
    await start('native', join(release, manifest.native.path), ['private-api','--oasis-demo-staging-config',privateConfig], {NODE_ENV:'production',PATH:process.env.PATH});
    for (const app of oasisDemoApps) {
      const reference = manifest.apps[app];
      await start(app, process.execPath, [join(release, reference.path, reference.entry)], {
        ...env, PORT:String(oasisDemoRouting.ports[app]), ERPX_PUBLIC_ORIGIN:app === 'account' ? accountOrigin : holdingOrigin,
      });
    }
    const expires = Date.now() + 30_000;
    while (!abort.signal.aborted && ![oasisDemoRouting.api, ...Object.values(oasisDemoRouting.ports)].every(hasPortListener)) {
      if (Date.now() >= expires) throw new Error('OASIS_STARTUP_TIMEOUT');
      await delay(250);
    }
    if (!abort.signal.aborted) {
      gateway = await startCompiledWorkspaceGateway({schema:oasisDemoRouting.schema}, oasisDemoRouting.gateway);
      console.log(`OASIS demo gateway is listening at http://127.0.0.1:${oasisDemoRouting.gateway}`);
      await stopped;
    }
    if (failed) throw new Error(`OASIS_PROCESS_EXITED:${failed}`);
  } finally {
    shuttingDown = true;
    const closed = Promise.all([...children.values()].map(child => child.exitCode !== null || child.signalCode !== null || !child.pid ? undefined : new Promise<void>(done => {
      child.once('exit', () => done());
      child.kill('SIGTERM');
    })));
    const force = setTimeout(() => { for (const child of children.values()) child.kill('SIGKILL'); }, 5_000);
    force.unref();
    await Promise.all([closed, gateway?.close()]);
    clearTimeout(force);
  }
}
