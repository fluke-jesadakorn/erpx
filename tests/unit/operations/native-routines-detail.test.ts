import {afterEach,describe,expect,it,vi} from 'vitest';
import {
  nativeRoutineTemplateDetail,
  nativeRoutineTemplateSchedules,
  type NativeRoutineTemplateRow,
} from '@erpx/operations-ui/public/native-routines-read';

let transportOverride: Parameters<typeof nativeRoutineTemplateDetail>[4];
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
const request = () => new Request('https://acme.jesadakorn.com/api/v1/holdings/acme/routine-templates/routine-1', {
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
  transportOverride = undefined;
});

describe('native routine-template detail has complete payload parity', () => {
  it('returns all fifteen retired row fields from the owner envelope', async () => {
    process.env.ERPX_NATIVE_OPERATIONS_ROUTINES_CONFIG = config;
    const transport = vi.fn<(url: string | URL | Request, init?: RequestInit) => Promise<Response>>(
      async () => reply(row),
    );
    transportOverride = transport;
    expect(await nativeRoutineTemplateDetail(request(), 'acme', 'routine-1', 'user-1', transportOverride)).toEqual(row);
    const [url, init] = transport.mock.calls[0]!;
    expect(String(url)).toBe('http://127.0.0.1:44993/api/v1/holdings/acme/routine-templates/routine-1');
    expect(init).toMatchObject({method: 'GET', cache: 'no-store'});
    expect(new Headers(init?.headers).get('x-erpx-context-user')).toBe('user-1');
  });

  it('returns null when the owner has no such template', async () => {
    process.env.ERPX_NATIVE_OPERATIONS_ROUTINES_CONFIG = config;
    transportOverride = vi.fn(async () => reply(null));
    expect(await nativeRoutineTemplateDetail(request(), 'acme', 'routine-1', 'user-1', transportOverride)).toBeNull();
  });

  it('fails closed on a changed context or a truncated row', async () => {
    process.env.ERPX_NATIVE_OPERATIONS_ROUTINES_CONFIG = config;
    transportOverride = vi.fn(async () => reply(row, {...context, revision: 8}));
    await expect(nativeRoutineTemplateDetail(request(), 'acme', 'routine-1', 'user-1', transportOverride))
      .rejects.toMatchObject({code: 'NATIVE_ROUTINES_RESPONSE_INVALID'});
    transportOverride = vi.fn(async () => reply({id: row.id}));
    await expect(nativeRoutineTemplateDetail(request(), 'acme', 'routine-1', 'user-1', transportOverride))
      .rejects.toMatchObject({code: 'NATIVE_ROUTINES_RESPONSE_INVALID'});
  });

  it('rejects an unbounded template id before any request', async () => {
    process.env.ERPX_NATIVE_OPERATIONS_ROUTINES_CONFIG = config;
    const transport = vi.fn();
    transportOverride = transport;
    await expect(nativeRoutineTemplateDetail(request(), 'acme', "x' OR 1=1", 'user-1', transportOverride))
      .rejects.toMatchObject({code: 'NATIVE_ROUTINES_RESPONSE_INVALID'});
    expect(transport).not.toHaveBeenCalled();
  });

  it('fails closed without the native configuration', async () => {
    delete process.env.ERPX_NATIVE_OPERATIONS_ROUTINES_CONFIG;
    const transport = vi.fn();
    transportOverride = transport;
    await expect(nativeRoutineTemplateDetail(request(), 'acme', 'routine-1', 'user-1', transportOverride))
      .rejects.toMatchObject({code: 'NATIVE_ROUTINES_UNAVAILABLE'});
    expect(transport).not.toHaveBeenCalled();
  });
});

it('binds the schedule read to the same template, Company and work selection', async () => {
  process.env.ERPX_NATIVE_OPERATIONS_ROUTINES_CONFIG = config;
  const schedule = {
    id: 'schedule-1', company_id: 'company', template_id: row.id, frequency: 'daily',
    timezone: 'Asia/Bangkok', occurrence_time: '09:00', start_date: '2026-11-27',
    end_date: null, weekday: null, month_day: null, due_offset_hours: 24, state: 'paused',
    next_occurrence_at: null, paused_at: 100, last_generated_at: null, last_error: null,
    version: 2, created_at: 100, updated_at: 200,
  };
  const data = {template: row, schedules: [schedule], canEdit: true};
  const transport = vi.fn<(url: string | URL | Request, init?: RequestInit) => Promise<Response>>(
    async () => reply(data),
  );
  transportOverride = transport;
  await expect(nativeRoutineTemplateSchedules(request(), 'acme', row.id, 'user-1', transportOverride)).resolves.toEqual(data);
  expect(String(transport.mock.calls[0]?.[0])).toMatch(/\/routine-templates\/routine-1\/schedules$/);

  transport.mockImplementation(async () => reply({...data, schedules: [{...schedule, company_id: 'other'}]}));
  await expect(nativeRoutineTemplateSchedules(request(), 'acme', row.id, 'user-1', transportOverride)).rejects.toMatchObject({
    code: 'NATIVE_ROUTINES_RESPONSE_INVALID',
  });
  transport.mockImplementation(async () => reply(data, {...context, revision: 8}));
  await expect(nativeRoutineTemplateSchedules(request(), 'acme', row.id, 'user-1', transportOverride)).rejects.toMatchObject({
    code: 'NATIVE_ROUTINES_RESPONSE_INVALID',
  });
});
