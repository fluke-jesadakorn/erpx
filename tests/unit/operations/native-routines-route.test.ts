import {afterEach,describe,expect,it,vi} from 'vitest';
import {nativeRoutinesAvailable} from '@erpx/server-operations/public/native-routines-page';
import {POST as templatesPost} from '../../../packages/feature-apps/src/routes/api/v1/holdings/[holdingSlug]/routine-templates/[[...path]]/route';
import {POST as schedulesPost} from '../../../packages/feature-apps/src/routes/api/v1/holdings/[holdingSlug]/routine-schedules/[[...path]]/route';

const retained = vi.hoisted(() => vi.fn(async () => Response.json({data:'retained'})));
vi.mock('@erpx/server-work_projection/public/staff-api', () => ({staffHandler: () => retained}));

const config = JSON.stringify({privateBaseUrl:'http://127.0.0.1:44993',timeoutMs:1000});
const request = (body: unknown = {}) =>
  new Request('https://acme.jesadakorn.com/api/v1/holdings/acme/routine-templates', {
    method: 'POST',
    headers: {'Content-Type': 'application/json', 'Idempotency-Key': 'routine-command-key'},
    body: JSON.stringify(body),
  });
const context = (path: string[]) => ({params: Promise.resolve({holdingSlug: 'acme', path})});
const scopedRequest = (body: unknown = {}) =>
  new Request('https://acme.jesadakorn.com/api/v1/holdings/acme/routine-templates', {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      'Idempotency-Key': 'routine-command-key',
      'x-erpx-context-company': 'company',
      'x-erpx-context-assignment': 'assignment',
      'x-erpx-context-revision': '7',
      'x-erpx-context-assignment-version': '3',
    },
    body: JSON.stringify(body),
  });
const ok = (data: unknown) =>
  new Response(JSON.stringify({data, context: {revision: 1}}), {
    status: 200,
    headers: {'content-type': 'application/json'},
  });

afterEach(() => {
  vi.unstubAllEnvs();
  vi.unstubAllGlobals();
  retained.mockClear();
});

describe('native routine routes are the sole command path', () => {
  it('fails closed without the native configuration instead of using a retained writer', async () => {
    delete process.env.ERPX_NATIVE_OPERATIONS_ROUTINES_CONFIG;
    expect(nativeRoutinesAvailable()).toBe(false);
    const templates = await templatesPost(request({title: 'Old writer'}), context([]));
    expect(templates.status).toBe(503);
    expect(await templates.json()).toMatchObject({error: {code: 'NATIVE_ROUTINES_UNAVAILABLE'}});
    const run = await templatesPost(request({dueAt: '2026-12-01T02:00:00Z'}), context(['template', 'run']));
    expect(run.status).toBe(503);
    expect(await run.json()).toMatchObject({error: {code: 'NATIVE_ROUTINES_UNAVAILABLE'}});
    const schedules = await schedulesPost(request({expectedVersion: 1}), context(['schedule', 'end']));
    expect(schedules.status).toBe(503);
    expect(await schedules.json()).toMatchObject({error: {code: 'NATIVE_ROUTINES_UNAVAILABLE'}});
    expect(retained).not.toHaveBeenCalled();
  });

  it('forwards every covered template and schedule shape with the command key', async () => {
    process.env.ERPX_NATIVE_OPERATIONS_ROUTINES_CONFIG = config;
    const transport = vi.fn<(url: string | URL | Request, init?: RequestInit) => Promise<Response>>(async () => ok({id: 'routine-receipt', version: 2}));
    vi.stubGlobal('fetch', transport);
    const cases = [
      [templatesPost, ['template', 'clone'], {}, '/routine-templates/template/clone'],
      [templatesPost, [], {title: 'Native create'}, '/routine-templates'],
      [templatesPost, ['template'], {expectedVersion: 1}, '/routine-templates/template'],
      [templatesPost, ['template', 'schedules'], {}, '/routine-templates/template/schedules'],
      [templatesPost, ['template', 'schedules', 'schedule'], {expectedVersion: 1}, '/routine-templates/template/schedules/schedule'],
      [schedulesPost, ['schedule', 'pause'], {expectedVersion: 1}, '/routine-schedules/schedule/pause'],
      [schedulesPost, ['schedule', 'resume'], {expectedVersion: 1}, '/routine-schedules/schedule/resume'],
      [schedulesPost, ['schedule', 'end'], {expectedVersion: 1}, '/routine-schedules/schedule/end'],
    ] as const;
    for (const [handler, path, body, suffix] of cases) {
      const response = await handler(request(body), context([...path]));
      expect(response.status).toBe(200);
      expect(await response.json()).toEqual({data: {id: 'routine-receipt', version: 2}});
      const [url, init] = transport.mock.calls.at(-1)!;
      expect(String(url)).toBe(`http://127.0.0.1:44993/api/v1/holdings/acme${suffix}`);
      expect(init).toMatchObject({method: 'POST', cache: 'no-store'});
      expect(new Headers(init?.headers).get('idempotency-key')).toBe('routine-command-key');
    }
    expect(transport).toHaveBeenCalledTimes(cases.length);
  });

  it('forwards the manual run and rejects a non-exact owner receipt or changed context', async () => {
    process.env.ERPX_NATIVE_OPERATIONS_ROUTINES_CONFIG = config;
    const runReceipt = {id: 'task-routine-command-key', state: 'start_approval', version: 1};
    const receiptContext = {companyId: 'company', assignmentId: 'assignment', revision: 7, assignmentVersion: 3};
    const transport = vi.fn<(url: string | URL | Request, init?: RequestInit) => Promise<Response>>(async () => new Response(JSON.stringify({data: runReceipt, context: receiptContext}), {status: 200, headers: {'content-type': 'application/json'}}));
    vi.stubGlobal('fetch', transport);
    const body = {dueAt: '2026-12-01T02:00:00Z'};
    const response = await templatesPost(scopedRequest(body), context(['template', 'run']));
    expect(response.status).toBe(200);
    expect(await response.json()).toEqual({data: runReceipt});
    const [url, init] = transport.mock.calls[0]!;
    expect(String(url)).toBe('http://127.0.0.1:44993/api/v1/holdings/acme/routine-templates/template/run');
    expect(init).toMatchObject({method: 'POST', cache: 'no-store'});
    expect(JSON.parse(String(init?.body))).toEqual(body);
    expect(new Headers(init?.headers).get('idempotency-key')).toBe('routine-command-key');
    for (const [data, replyContext] of [
      [{...runReceipt, version: 2}, receiptContext],
      [{...runReceipt, id: 'task-other'}, receiptContext],
      [{...runReceipt, state: 'completed'}, receiptContext],
      [runReceipt, {...receiptContext, revision: 8}],
    ] as const) {
      vi.stubGlobal('fetch', vi.fn(async () => new Response(JSON.stringify({data, context: replyContext}), {status: 200, headers: {'content-type': 'application/json'}})));
      const invalid = await templatesPost(scopedRequest(body), context(['template', 'run']));
      expect(invalid.status).toBe(503);
      expect(await invalid.json()).toMatchObject({error: {code: 'NATIVE_ROUTINES_RESPONSE_INVALID'}});
    }
    expect(retained).not.toHaveBeenCalled();
  });

  it('keeps the retained read shape off the native command transport', async () => {
    process.env.ERPX_NATIVE_OPERATIONS_ROUTINES_CONFIG = config;
    const transport = vi.fn();
    vi.stubGlobal('fetch', transport);
    expect(await (await schedulesPost(request({dueAt: '2026-12-01T02:00:00Z'}), context(['schedule', 'unknown']))).json())
      .toEqual({data: 'retained'});
    expect(retained).toHaveBeenCalledTimes(1);
    expect(transport).not.toHaveBeenCalled();
  });

  it('rejects a native configuration that can escape the loopback service', async () => {
    process.env.ERPX_NATIVE_OPERATIONS_ROUTINES_CONFIG = JSON.stringify({
      privateBaseUrl: 'http://127.0.0.1:3100',
      timeoutMs: 1000,
    });
    const response = await templatesPost(request({title: 'Native create'}), context([]));
    expect(response.status).toBe(503);
    expect(await response.json()).toMatchObject({error: {code: 'NATIVE_ROUTINES_CONFIGURATION_INVALID'}});
    expect(retained).not.toHaveBeenCalled();
  });
});
