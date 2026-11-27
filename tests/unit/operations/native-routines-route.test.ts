import { existsSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { afterEach, expect, it, vi } from 'vitest';
import { holdingNativeCutoverPath, normalizeHoldingNativePath } from '../../../scripts/local/native-cutover-routes';
import { POST as schedulesPost } from '../../../packages/feature-apps/src/routes/api/v1/holdings/[holdingSlug]/routine-schedules/[[...path]]/route';

const retained = vi.hoisted(() => vi.fn(async () => Response.json({data:'retained'})));
vi.mock('@erpx/server-work_projection/public/staff-api', () => ({staffHandler: () => retained}));

const root = resolve(__dirname, '../../..');
const base = '/api/v1/holdings/acme/routine-templates';
const scheduleRequest = () => new Request('https://acme.jesadakorn.com/api/v1/holdings/acme/routine-schedules', {
  method: 'POST',
  headers: {'Content-Type': 'application/json', 'Idempotency-Key': 'routine-command-key'},
  body: '{"expectedVersion":1}',
});
const scheduleContext = (operation: string) => ({params: Promise.resolve({holdingSlug: 'acme', path: ['schedule', operation]})});

afterEach(() => {
  vi.unstubAllEnvs();
  vi.unstubAllGlobals();
  retained.mockClear();
});

it('retires the full Next route and sends scoped template commands to Axum', () => {
  const route = 'api/v1/holdings/[holdingSlug]/routine-templates/[[...path]]/route';
  for (const directory of ['packages/feature-apps/src/routes', 'apps/launcher/src/app', 'apps/compat/src/app'])
    expect(existsSync(resolve(root, directory, route + '.ts'))).toBe(false);
  const manifest = JSON.parse(readFileSync(resolve(root, 'packages/feature-apps/package.json'), 'utf8'));
  expect(manifest.exports['./routes/' + route]).toBeUndefined();
  expect(holdingNativeCutoverPath(base, 'acme', 'POST')).toBe('routine-command');
  for (const suffix of ['/template', '/template/clone', '/template/run', '/template/schedules', '/template/schedules/schedule']) {
    expect(holdingNativeCutoverPath(base + suffix, 'acme', 'POST')).toBe('routine-command');
    expect(holdingNativeCutoverPath(base + suffix, 'acme', 'GET')).toBe('routine-templates');
  }
  const scoped = normalizeHoldingNativePath('/api/v1/holdings/acme/branches/bangkok/departments/ops/routine-templates/template/run', 'acme');
  expect(scoped).toEqual({url: base + '/template/run', workspace: {branch: 'bangkok', department: 'ops'}});
  expect(holdingNativeCutoverPath(scoped.url, 'acme', 'POST')).toBe('routine-command');
  expect(holdingNativeCutoverPath(base + '/template/schedules/schedule', 'northstar', 'GET')).toBeUndefined();
});

it('forwards the existing schedule state commands with their exact keys', async () => {
  vi.stubEnv('ERPX_NATIVE_OPERATIONS_ROUTINES_CONFIG', JSON.stringify({privateBaseUrl:'http://127.0.0.1:44993',timeoutMs:1000}));
  const transport = vi.fn<(url: string | URL | Request, init?: RequestInit) => Promise<Response>>(
    async () => Response.json({data: {id: 'schedule', version: 2}, context: {revision: 1}}),
  );
  vi.stubGlobal('fetch', transport);
  for (const action of ['pause', 'resume', 'end']) {
    const response = await schedulesPost(scheduleRequest(), scheduleContext(action));
    expect(response.status).toBe(200);
    expect(await response.json()).toEqual({data: {id: 'schedule', version: 2}});
    const [url, init] = transport.mock.calls.at(-1)!;
    expect(String(url)).toBe(`http://127.0.0.1:44993/api/v1/holdings/acme/routine-schedules/schedule/${action}`);
    expect(init).toMatchObject({method: 'POST', cache: 'no-store'});
    expect(new Headers(init?.headers).get('idempotency-key')).toBe('routine-command-key');
  }
  expect(transport).toHaveBeenCalledTimes(3);
  expect(retained).not.toHaveBeenCalled();
});

it('fails existing schedule writes closed without a usable native base', async () => {
  for (const native of [undefined, JSON.stringify({privateBaseUrl:'https://external.example.test',timeoutMs:1000})]) {
    vi.stubEnv('ERPX_NATIVE_OPERATIONS_ROUTINES_CONFIG', native);
    const response = await schedulesPost(scheduleRequest(), scheduleContext('end'));
    expect(response.status).toBe(503);
  }
  expect(retained).not.toHaveBeenCalled();
  const unrelated = await schedulesPost(scheduleRequest(), scheduleContext('unknown'));
  expect(await unrelated.json()).toEqual({data:'retained'});
});
