import {afterEach,describe,expect,it,vi} from 'vitest';
import {
  nativeRoutineTemplateList,
  nativeRoutineTemplatesAvailable,
  type NativeRoutineTemplateRow,
} from '@erpx/feature-operations/public/native-routines-read';

const config = JSON.stringify({privateBaseUrl: 'http://127.0.0.1:44993', timeoutMs: 1000});
const row: NativeRoutineTemplateRow = {
  id: 'routine-1',
  title: 'Weekly close',
  description: 'Close the week',
  company_id: 'company',
  branch_id: null,
  department_id: null,
  creator_user_id: 'user-1',
  assignee_user_id: 'user-2',
  project_id: null,
  start_approvers_json: '[]',
  completion_approvers_json: '[]',
  published: 1,
  version: 2,
  created_at: 100,
  updated_at: 200,
};
const context = {
  companyId: 'company', userId: 'user-1', assignmentId: 'assignment', revision: 7,
  assignmentVersion: 3,
};
const request = () => new Request('https://acme.jesadakorn.com/api/v1/holdings/acme/routine-templates', {
  headers: {
    host: 'acme.jesadakorn.com',
    'x-erpx-context-company': 'company',
    'x-erpx-context-assignment': 'assignment',
    'x-erpx-context-revision': '7',
    'x-erpx-context-assignment-version': '3',
  },
});
const reply = (data: unknown, replyContext: unknown = context) =>
  new Response(JSON.stringify({data, context: replyContext}), {
    status: 200, headers: {'content-type': 'application/json'},
  });

afterEach(() => {
  vi.unstubAllEnvs();
  vi.unstubAllGlobals();
});

describe('native routine-template list has complete payload parity', () => {
  it('returns all fifteen retired row fields from the owner envelope', async () => {
    process.env.ERPX_NATIVE_OPERATIONS_ROUTINES_CONFIG = config;
    expect(nativeRoutineTemplatesAvailable()).toBe(true);
    const transport = vi.fn<(url: string | URL | Request, init?: RequestInit) => Promise<Response>>(
      async () => reply([row]),
    );
    vi.stubGlobal('fetch', transport);
    expect(await nativeRoutineTemplateList(request(), 'acme', 'user-1')).toEqual([row]);
    const [url, init] = transport.mock.calls[0]!;
    expect(String(url)).toBe('http://127.0.0.1:44993/api/v1/holdings/acme/routine-templates');
    expect(init).toMatchObject({method: 'GET', cache: 'no-store'});
    expect(new Headers(init?.headers).get('x-erpx-context-user')).toBe('user-1');
  });

  it('fails closed on a changed context or a truncated row', async () => {
    process.env.ERPX_NATIVE_OPERATIONS_ROUTINES_CONFIG = config;
    vi.stubGlobal('fetch', vi.fn(async () => reply([row], {...context, revision: 8})));
    await expect(nativeRoutineTemplateList(request(), 'acme', 'user-1'))
      .rejects.toMatchObject({code: 'NATIVE_ROUTINES_RESPONSE_INVALID'});
    vi.stubGlobal('fetch', vi.fn(async () => reply([{id: row.id}])));
    await expect(nativeRoutineTemplateList(request(), 'acme', 'user-1'))
      .rejects.toMatchObject({code: 'NATIVE_ROUTINES_RESPONSE_INVALID'});
  });

  it('fails closed without the native configuration', async () => {
    delete process.env.ERPX_NATIVE_OPERATIONS_ROUTINES_CONFIG;
    expect(nativeRoutineTemplatesAvailable()).toBe(false);
    const transport = vi.fn();
    vi.stubGlobal('fetch', transport);
    await expect(nativeRoutineTemplateList(request(), 'acme', 'user-1'))
      .rejects.toMatchObject({code: 'NATIVE_ROUTINES_UNAVAILABLE'});
    expect(transport).not.toHaveBeenCalled();
  });
});
