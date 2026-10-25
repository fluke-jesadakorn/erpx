import { afterEach, expect, it } from 'vitest';
import { access, mkdtemp, readFile, writeFile, rm, mkdir, cp, realpath } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { tmpdir } from 'node:os';
import { resolve, join } from 'node:path';
import { pathToFileURL } from 'node:url';
import { spawn } from 'node:child_process';
import { browserChildEnvironment } from "../../../scripts/testing/runner-environment";
// This Node preload is deliberately JavaScript so the compiled cohort can import it unchanged.
import { createProviderFixtureFetch, assertProviderFixtureScope } from "../../../scripts/local/pms-provider-fixture.mjs";

const homes: string[] = [];
afterEach(async () => { for (const home of homes.splice(0)) await rm(home, { recursive: true, force: true }); });
async function fixture(extra: Record<string, unknown> = {}) {
  const home = await realpath(await mkdtemp(join(tmpdir(), 'erpx-fictional-provider-'))); homes.push(home);
  await writeFile(join(home, 'provider-fixture.json'), JSON.stringify({ schemaVersion: 1, keys: {}, resources: {}, requests: [], ...extra }));
  return home;
}
const options = (key = 'original-key', amount = '21400') => ({ method: 'POST', headers: {
  Authorization: 'Bearer sk_test_erpx_fictional_provider', 'Stripe-Account': 'acct_erpx_fictional', 'Idempotency-Key': key,
}, body: new URLSearchParams({ amount, 'metadata[pms_booking_refund_id]': 'fictional-refund' }) });
const noNetwork = () => { throw new Error('Provider fixture must not call the network'); };

it('retains one accepted refund across lost response and a new transport instance', async () => {
  const home = await fixture({ loseNextResponse: 'refunds' });
  const first = await createProviderFixtureFetch(home, noNetwork);
  await expect(first('https://api.stripe.com/v1/refunds', options())).rejects.toThrow('response connection lost');
  const next = await createProviderFixtureFetch(home, noNetwork), response = await next('https://api.stripe.com/v1/refunds', options());
  expect(await response.json()).toMatchObject({ id: 're_fictional_1', amount: 21400, status: 'succeeded' });
  const state = JSON.parse(await readFile(join(home, 'provider-fixture.json'), 'utf8'));
  expect(Object.keys(state.keys)).toHaveLength(1); expect(Object.keys(state.resources)).toHaveLength(1);
  expect(state.requests).toHaveLength(2);
  await expect(next('https://api.stripe.com/v1/refunds', options('original-key', '21500'))).rejects.toThrow('IDEMPOTENCY_MISMATCH');
});

it('replays durable provider acceptance after killing the process waiting for its response', async () => {
  const home = await fixture({ holdNextResponse: 'refunds' });
  const moduleUrl = pathToFileURL(resolve('scripts/local/pms-provider-fixture.mjs')).href;
  const code = `import {createProviderFixtureFetch} from ${JSON.stringify(moduleUrl)};const fetch=await createProviderFixtureFetch(${JSON.stringify(home)},()=>{throw new Error('No network')});await fetch('https://api.stripe.com/v1/refunds',${JSON.stringify({ ...options(), body: options().body.toString() })});`;
  const child = spawn(process.execPath, ['--input-type=module', '-e', code], { stdio: 'ignore' });
  const ended = new Promise(resolveExit => child.once('exit', resolveExit));
  try {
    await expect.poll(async () => JSON.parse(await readFile(join(home, 'provider-fixture.json'), 'utf8')).held?.pid).toBe(child.pid);
    // The durable record precedes lock release. Kill only in the response wait,
    // after the provider has finished its mutation and released the write lock.
    await expect.poll(async () => {
      try { await access(join(home, 'provider-fixture.lock')); return false; }
      catch (error) { if ((error as NodeJS.ErrnoException).code !== 'ENOENT') throw error; return true; }
    }).toBe(true);
    child.kill('SIGKILL'); await ended;
    const next = await createProviderFixtureFetch(home, noNetwork);
    expect(await (await next('https://api.stripe.com/v1/refunds', options())).json()).toMatchObject({ id: 're_fictional_1', amount: 21400 });
    const state = JSON.parse(await readFile(join(home, 'provider-fixture.json'), 'utf8'));
    expect(Object.keys(state.keys)).toHaveLength(1); expect(state.requests).toHaveLength(2);
  } finally { if (child.exitCode === null && child.signalCode === null) { child.kill('SIGKILL'); await ended; } }
});

it('holds concurrent reads of the same accepted payment until the response fault is released', async () => {
  const home = await fixture({ holdNextResponse: 'payment_intents', resources: { pi_fictional: { id: 'pi_fictional', status: 'succeeded', amount_received: 1412400 } } });
  const fetch = await createProviderFixtureFetch(home, noNetwork), request = { headers: options().headers };
  const first = fetch('https://api.stripe.com/v1/payment_intents/pi_fictional', request);
  await expect.poll(async () => JSON.parse(await readFile(join(home, 'provider-fixture.json'), 'utf8')).held?.pid).toBe(process.pid);
  let secondFinished = false;
  const second = fetch('https://api.stripe.com/v1/payment_intents/pi_fictional', request).then((response: Response) => { secondFinished = true; return response; });
  await expect.poll(async () => JSON.parse(await readFile(join(home, 'provider-fixture.json'), 'utf8')).requests.length).toBe(2);
  expect(secondFinished).toBe(false);
  const state = JSON.parse(await readFile(join(home, 'provider-fixture.json'), 'utf8')); state.held = null;
  await writeFile(join(home, 'provider-fixture.json'), JSON.stringify(state));
  for (const response of await Promise.all([first, second])) expect(await response.json()).toMatchObject({ id: 'pi_fictional', status: 'succeeded' });
});

it('rejects live or mismatched credentials without provider network access and delegates unrelated fetches', async () => {
  const home = await fixture(), seen: unknown[] = [];
  const fetch = await createProviderFixtureFetch(home, async (input: unknown) => { seen.push(input); return Response.json({ unrelated: true }); });
  const bad = { ...options(), headers: { ...options().headers, Authorization: 'Bearer sk_live_forbidden' } };
  await expect(fetch('https://api.stripe.com/v1/refunds', bad)).rejects.toThrow('CREDENTIALS_REQUIRED');
  expect(seen).toHaveLength(0); expect(await (await fetch('http://127.0.0.1:3104/api/health')).json()).toEqual({ unrelated: true });
  expect(seen).toEqual(['http://127.0.0.1:3104/api/health']);
});

it('refuses activation outside the exact compiled booking cohort before installing a fetch override', async () => {
  await expect(assertProviderFixtureScope({ ERPX_PMS_PROVIDER_FIXTURE: '1', ERPX_BROWSER_SIMULATION_PORT: '3101' })).rejects.toThrow('EXACT_PMS_PROVIDER_FIXTURE_REQUIRED');
  await expect(assertProviderFixtureScope({ ERPX_PMS_PROVIDER_FIXTURE: '1', ERPX_BROWSER_SIMULATION_RUN_ID: 'abcdef123456', ERPX_BROWSER_SIMULATION_PORT: '3104', ERPX_BROWSER_SIMULATION: 'thai-hotel-v1', ERPX_PUBLIC_ORIGIN: 'http://127.0.0.1:3104', POS_STRIPE_TEST_SECRET_KEY: 'sk_test_erpx_fictional_provider', POS_STRIPE_LIVE_SECRET_KEY: 'sk_live_forbidden' })).rejects.toThrow('EXACT_PMS_PROVIDER_FIXTURE_REQUIRED');
});

it('strips ambient provider activation and forwards only explicitly scoped fictional test credentials', () => {
  const ambient = { ERPX_PMS_PROVIDER_FIXTURE: '1', PMS_STRIPE_BOOKING_ENABLED: 'true', POS_STRIPE_TEST_SECRET_KEY: 'sk_test_ambient', POS_STRIPE_LIVE_SECRET_KEY: 'sk_live_ambient' };
  expect(browserChildEnvironment(ambient)).not.toHaveProperty('ERPX_PMS_PROVIDER_FIXTURE');
  expect(browserChildEnvironment(ambient)).not.toHaveProperty('PMS_STRIPE_BOOKING_ENABLED');
  expect(browserChildEnvironment(ambient)).not.toHaveProperty('POS_STRIPE_TEST_SECRET_KEY');
  const overrides = { ERPX_PMS_PROVIDER_FIXTURE: '1', ERPX_ENVIRONMENT: 'local', ERPX_PUBLIC_ORIGIN: 'http://127.0.0.1:3104', ERPX_DATA_DIR: '/repo/.runtime/browser/abcdef123456', POS_STRIPE_TEST_SECRET_KEY: 'sk_test_erpx_fictional_provider', POS_STRIPE_TEST_WEBHOOK_SECRET: 'whsec_erpx_fictional_provider' };
  expect(browserChildEnvironment(ambient, overrides)).toMatchObject({ ERPX_PMS_PROVIDER_FIXTURE: '1', PMS_STRIPE_BOOKING_ENABLED: 'true', POS_STRIPE_TEST_SECRET_KEY: 'sk_test_erpx_fictional_provider' });
  expect(browserChildEnvironment(ambient, overrides)).not.toHaveProperty('POS_STRIPE_LIVE_SECRET_KEY');
  expect(() => browserChildEnvironment(ambient, { ...overrides, ERPX_PUBLIC_ORIGIN: 'http://127.0.0.1:3101' })).toThrow('EXACT_FICTIONAL_BROWSER_PROVIDER_REQUIRED');
});

it('binds activation to the owned manifest, compiled release and unchanged preload bytes', async () => {
  const home = await fixture(), run = 'abcdef123456', directory = join(home, '.runtime/browser', run), release = join(directory, 'release');
  await mkdir(join(release, 'tools'), { recursive: true });
  const preload = join(release, 'tools/pms-provider-fixture.mjs');
  await cp(resolve('scripts/local/pms-provider-fixture.mjs'), preload);
  const hash = (bytes: string | Buffer) => createHash('sha256').update(bytes).digest('hex');
  const files = JSON.stringify({ 'tools/pms-provider-fixture.mjs': hash(await readFile(preload)) });
  await writeFile(join(release, 'release-files.json'), files);
  const identity = { id: 'fictional-unit-release', sourceFingerprint: 'a'.repeat(64), fileManifestSha256: hash(files), node: '26.11.1' };
  await writeFile(join(release, 'release.json'), JSON.stringify(identity));
  await writeFile(join(directory, 'state.json'), JSON.stringify({ run, prefix: `erpx_browser_${run}`, profile: 'booking', port: 3104, origin: 'http://127.0.0.1:3104', ready: true, providerFixture: true, release: identity, sourceFingerprint: identity.sourceFingerprint }));
  const bindings = ['CONTROL_DB', 'CONTROL_READ_DB', 'H001_DB', 'H001_READ_DB', 'H002_DB', 'H002_READ_DB', 'H003_DB', 'H003_READ_DB'];
  const manifest = { version: 1, databases: Object.fromEntries(bindings.map(binding => [binding, `postgresql://erpx_test_admin@127.0.0.1:5432/erpx_browser_${run}_${binding.toLowerCase()}`])) };
  await writeFile(join(directory, 'databases.json'), JSON.stringify(manifest));
  const environment = { ERPX_PMS_PROVIDER_FIXTURE: '1', ERPX_BROWSER_SIMULATION_RUN_ID: run, ERPX_BROWSER_SIMULATION_PORT: '3104', ERPX_BROWSER_SIMULATION: 'thai-hotel-v1', ERPX_PUBLIC_ORIGIN: 'http://127.0.0.1:3104', POS_STRIPE_TEST_SECRET_KEY: 'sk_test_erpx_fictional_provider', ERPX_DATA_DIR: directory, ERPX_DATABASE_MANIFEST: join(directory, 'databases.json') };
  expect(await assertProviderFixtureScope(environment, release)).toBe(directory);
  await expect(assertProviderFixtureScope(environment, home)).rejects.toThrow('DIRECTORY_REQUIRED');
  manifest.databases.H001_DB = 'postgresql://erpx_test_admin@127.0.0.1:5432/erpx_h001';
  await writeFile(join(directory, 'databases.json'), JSON.stringify(manifest));
  await expect(assertProviderFixtureScope(environment, release)).rejects.toThrow('DATABASES_REQUIRED');
  manifest.databases.H001_DB = `postgresql://erpx_test_admin@127.0.0.1:5432/erpx_browser_${run}_h001_db`;
  await writeFile(join(directory, 'databases.json'), JSON.stringify(manifest));
  await writeFile(preload, 'Changed after release');
  await expect(assertProviderFixtureScope(environment, release)).rejects.toThrow('CHECKSUM_MISMATCH');
});
