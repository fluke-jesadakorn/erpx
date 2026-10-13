/** Fictional provider transport for an explicitly owned compiled booking cohort.
 * Never an application adapter or provider acceptance. No provider network calls.
 */
import { createHash } from 'node:crypto';
import { readFile, writeFile, rename, mkdir, rm, realpath } from 'node:fs/promises';
import { join, resolve } from 'node:path';

const json = async path => JSON.parse(await readFile(path, 'utf8'));
const digest = value => createHash('sha256').update(value).digest('hex');
const sleep = ms => new Promise(ok => setTimeout(ok, ms));
const processAlive = pid => {
  try { process.kill(pid, 0); return true; }
  catch (error) { if (error.code === 'ESRCH') return false; throw error; }
};

export async function assertProviderFixtureScope(environment, cwd = process.cwd()) {
  const run = environment.ERPX_BROWSER_SIMULATION_RUN_ID;
  if (environment.ERPX_PMS_PROVIDER_FIXTURE !== '1' || !/^[a-f0-9]{12}$/.test(run ?? '') ||
      environment.ERPX_BROWSER_SIMULATION_PORT !== '3104' ||
      environment.ERPX_BROWSER_SIMULATION !== 'thai-hotel-v1' ||
      environment.ERPX_PUBLIC_ORIGIN !== 'http://127.0.0.1:3104' ||
      environment.POS_STRIPE_TEST_SECRET_KEY !== 'sk_test_erpx_fictional_provider' ||
      environment.POS_STRIPE_LIVE_SECRET_KEY) throw new Error('EXACT_PMS_PROVIDER_FIXTURE_REQUIRED');
  const directory = await realpath(environment.ERPX_DATA_DIR ?? '');
  if (!directory.endsWith(`/.runtime/browser/${run}`) || await realpath(cwd) !== join(directory, 'release') ||
      resolve(environment.ERPX_DATABASE_MANIFEST ?? '') !== join(directory, 'databases.json'))
    throw new Error('EXACT_PMS_PROVIDER_DIRECTORY_REQUIRED');
  const state = await json(join(directory, 'state.json'));
  if (state.run !== run || state.prefix !== `erpx_browser_${run}` || state.profile !== 'booking' ||
      state.port !== 3104 || state.origin !== environment.ERPX_PUBLIC_ORIGIN || !state.ready ||
      state.providerFixture !== true || !state.release) throw new Error('EXACT_PMS_PROVIDER_COHORT_REQUIRED');
  const manifest = await json(join(directory, 'databases.json'));
  const bindings = ['CONTROL_DB', 'CONTROL_READ_DB', 'H001_DB', 'H001_READ_DB', 'H002_DB', 'H002_READ_DB', 'H003_DB', 'H003_READ_DB'];
  if (manifest.version !== 1 || Object.keys(manifest.databases ?? {}).length !== bindings.length)
    throw new Error('EXACT_PMS_PROVIDER_DATABASES_REQUIRED');
  for (const binding of bindings) {
    const url = new URL(manifest.databases[binding]);
    if (url.hostname !== '127.0.0.1' || url.port !== '5432' || url.username !== 'erpx_test_admin' ||
        decodeURIComponent(url.pathname) !== `/erpx_browser_${run}_${binding.toLowerCase()}`)
      throw new Error('EXACT_PMS_PROVIDER_DATABASES_REQUIRED');
  }
  const release = await json(join(directory, 'release/release.json'));
  const files = await readFile(join(directory, 'release/release-files.json'));
  if (release.node !== '26.11.1' || !/^[a-f0-9]{64}$/.test(state.sourceFingerprint ?? '') ||
      release.sourceFingerprint !== state.sourceFingerprint || release.id !== state.release.id ||
      release.fileManifestSha256 !== state.release.fileManifestSha256 || digest(files) !== release.fileManifestSha256)
    throw new Error('EXACT_PMS_PROVIDER_RELEASE_REQUIRED');
  const fileDigests = JSON.parse(files);
  if (fileDigests['tools/pms-provider-fixture.mjs'] !== digest(await readFile(join(directory, 'release/tools/pms-provider-fixture.mjs'))))
    throw new Error('PMS_PROVIDER_PRELOAD_CHECKSUM_MISMATCH');
  return directory;
}

/** Atomic file state survives killing/replacing web/jobs; no request contains credentials. */
export async function createProviderFixtureFetch(directory, originalFetch) {
  const path = join(directory, 'provider-fixture.json'), lock = join(directory, 'provider-fixture.lock');
  async function mutate(action) {
    const deadline = Date.now() + 5000;
    for (;;) {
      try { await mkdir(lock); break; }
      catch (error) {
        if (error.code !== 'EEXIST' || Date.now() > deadline) throw error;
        await sleep(10);
      }
    }
    try {
      const state = await json(path), result = action(state);
      const temporary = `${path}.${process.pid}.tmp`;
      await writeFile(temporary, JSON.stringify(state) + '\n', { mode: 0o600 });
      await rename(temporary, path);
      return result;
    } finally { await rm(lock, { recursive: true }); }
  }
  return async (input, options = {}) => {
    const url = new URL(typeof input === 'string' || input instanceof URL ? input : input.url);
    if (url.hostname !== 'api.stripe.com') return originalFetch(input, options);
    const headers = new Headers(options.headers ?? (input instanceof Request ? input.headers : undefined));
    if (url.origin !== 'https://api.stripe.com' || headers.get('authorization') !== 'Bearer sk_test_erpx_fictional_provider' ||
        headers.get('stripe-account') !== 'acct_erpx_fictional') throw new Error('FICTIONAL_PROVIDER_CREDENTIALS_REQUIRED');
    const method = options.method ?? 'GET', route = url.pathname.replace(/^\/v1\//, '');
    const body = new URLSearchParams(options.body ?? ''), key = headers.get('idempotency-key');
    const result = await mutate(state => {
      if (state.schemaVersion !== 1) throw new Error('FICTIONAL_PROVIDER_STATE_REQUIRED');
      if (state.held && !processAlive(state.held.pid)) state.held = null;
      state.requests.push({ method, route, key, pid: process.pid });
      if (method === 'POST') {
        if (!key) throw new Error('FICTIONAL_PROVIDER_IDEMPOTENCY_REQUIRED');
        const signature = digest(`${route}\n${body}`), prior = state.keys[key];
        if (prior) {
          if (prior.signature !== signature) throw new Error('FICTIONAL_PROVIDER_IDEMPOTENCY_MISMATCH');
          return { resource: state.resources[prior.id], replay: true, hold: state.held?.route === route && state.held.key === key };
        }
        const id = `${route === 'refunds' ? 're' : route === 'checkout/sessions' ? 'cs' : 'pi'}_fictional_${Object.keys(state.keys).length + 1}`;
        const metadata = Object.fromEntries([...body].filter(([name]) => name.startsWith('metadata[')).map(([name, value]) => [name.slice(9, -1), value]));
        let resource;
        if (route === 'refunds') resource = { id, object: 'refund', amount: Number(body.get('amount')), currency: 'thb', status: 'succeeded', metadata };
        else if (route === 'checkout/sessions') {
          const paymentIntent = `pi_${id}`, amount = Number(body.get('line_items[0][price_data][unit_amount]'));
          state.resources[paymentIntent] = { id: paymentIntent, object: 'payment_intent', amount, amount_received: amount, currency: 'thb', status: 'succeeded', metadata, created: Math.floor(Date.now() / 1000), latest_charge: null };
          resource = { id, object: 'checkout.session', url: `https://checkout.stripe.com/c/pay/${id}`, payment_status: 'paid', status: 'complete', payment_intent: paymentIntent, amount_total: amount, currency: 'thb', metadata };
        } else throw new Error('FICTIONAL_PROVIDER_ROUTE_UNSUPPORTED');
        state.resources[id] = resource; state.keys[key] = { signature, id };
        const lose = state.loseNextResponse === route, hold = state.holdNextResponse === route;
        if (lose) state.loseNextResponse = null;
        if (hold) { state.holdNextResponse = null; state.held = { route, key, pid: process.pid }; }
        return { resource, lose, hold };
      }
      const id = route.split('/').at(-1), resource = state.resources[id];
      if (!resource) throw new Error('FICTIONAL_PROVIDER_RESOURCE_MISSING');
      const hold = state.held?.route === route || state.holdNextResponse === route || state.holdNextResponse === route.split('/')[0];
      if (hold && !state.held) { state.holdNextResponse = null; state.held = { route, key, pid: process.pid }; }
      return { resource, hold };
    });
    // Hold only after durable acceptance and releasing the lock. Worker termination
    // cannot strand the provider lock or erase its accepted idempotency result.
    if (result.hold) while ((await json(path)).held) {
      options.signal?.throwIfAborted(); await sleep(50);
    }
    if (result.lose) throw new TypeError('Fictional provider accepted request; response connection lost');
    return Response.json(result.resource);
  };
}

if (process.env.ERPX_PMS_PROVIDER_FIXTURE === '1') {
  const directory = await assertProviderFixtureScope(process.env);
  globalThis.fetch = await createProviderFixtureFetch(directory, globalThis.fetch.bind(globalThis));
}
