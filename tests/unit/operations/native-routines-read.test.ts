import {createServer} from 'node:http';
import {afterEach,describe,expect,it,vi} from 'vitest';
import {
  nativeRoutineEditorContext,
  nativeRoutineTemplateList,
  nativeRoutineTemplatesAvailable,
  type NativeRoutineEditorContext,
  type NativeRoutineTemplateRow,
} from '@erpx/feature-operations/public/native-routines-read';

let transportOverride: Parameters<typeof nativeRoutineTemplateList>[3];
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
const purchase = {newWork: true, settleExisting: false, history: true, revision: 'purchase-revision'};
const editor: NativeRoutineEditorContext = {
  projects: [{id: 'project', title: 'Reviewed project'}],
  options: {
    people: [{user_id: 'user-1', legal_name: 'Worker', department_id: 'department', level_id: 'level', level_name: 'Engineer'}],
    levels: [{id: 'level', name: 'Engineer', rank: 4}],
    departments: [{id: 'department', display_name: 'Engineering'}],
  },
  canWrite: true,
  purchases: {work: purchase, projects: {...purchase, newWork: false}, context: 'purchase-context'},
};
const editorRequest = () => new Request('https://acme.jesadakorn.com/api/v1/holdings/acme/routine-template-editor-context', {
  headers: {
    host: 'acme.jesadakorn.com',
    'x-erpx-context-company': 'company',
    'x-erpx-context-assignment': 'assignment',
    'x-erpx-context-revision': '7',
    'x-erpx-context-assignment-version': '3',
  },
});

afterEach(() => {
  vi.unstubAllEnvs();
  transportOverride = undefined;
});

describe('native routine-template list has complete payload parity', () => {
  it('returns all fifteen retired row fields from the owner envelope', async () => {
    process.env.ERPX_NATIVE_OPERATIONS_ROUTINES_CONFIG = config;
    expect(nativeRoutineTemplatesAvailable()).toBe(true);
    const transport = vi.fn<(url: string | URL | Request, init?: RequestInit) => Promise<Response>>(
      async () => reply([row]),
    );
    transportOverride = transport;
    expect(await nativeRoutineTemplateList(request(), 'acme', 'user-1', transportOverride)).toEqual([row]);
    const [url, init] = transport.mock.calls[0]!;
    expect(String(url)).toBe('http://127.0.0.1:44993/api/v1/holdings/acme/routine-templates');
    expect(init).toMatchObject({method: 'GET', cache: 'no-store'});
    expect(new Headers(init?.headers).get('x-erpx-context-user')).toBe('user-1');
  });

  it('fails closed on a changed context or a truncated row', async () => {
    process.env.ERPX_NATIVE_OPERATIONS_ROUTINES_CONFIG = config;
    transportOverride = vi.fn(async () => reply([row], {...context, revision: 8}));
    await expect(nativeRoutineTemplateList(request(), 'acme', 'user-1', transportOverride))
      .rejects.toMatchObject({code: 'NATIVE_ROUTINES_RESPONSE_INVALID'});
    transportOverride = vi.fn(async () => reply([{id: row.id}]));
    await expect(nativeRoutineTemplateList(request(), 'acme', 'user-1', transportOverride))
      .rejects.toMatchObject({code: 'NATIVE_ROUTINES_RESPONSE_INVALID'});
  });

  it('fails closed without the native configuration', async () => {
    delete process.env.ERPX_NATIVE_OPERATIONS_ROUTINES_CONFIG;
    expect(nativeRoutineTemplatesAvailable()).toBe(false);
    const transport = vi.fn();
    transportOverride = transport;
    await expect(nativeRoutineTemplateList(request(), 'acme', 'user-1', transportOverride))
      .rejects.toMatchObject({code: 'NATIVE_ROUTINES_UNAVAILABLE'});
    expect(transport).not.toHaveBeenCalled();
  });
});

describe('native routine editor context has complete payload parity', () => {
  it('returns every native editor field from the private envelope on the editor context path', async () => {
    process.env.ERPX_NATIVE_OPERATIONS_ROUTINES_CONFIG = config;
    const transport = vi.fn<(url: string | URL | Request, init?: RequestInit) => Promise<Response>>(
      async () => reply(editor),
    );
    transportOverride = transport;
    expect(await nativeRoutineEditorContext(editorRequest(), 'acme', 'user-1', transportOverride)).toEqual(editor);
    const [url, init] = transport.mock.calls[0]!;
    expect(String(url)).toBe('http://127.0.0.1:44993/api/v1/holdings/acme/routine-template-editor-context');
    expect(init).toMatchObject({method: 'GET', cache: 'no-store'});
    expect(new Headers(init?.headers).get('x-erpx-context-user')).toBe('user-1');
  });

  it('keeps a null purchase projection null for the caller', async () => {
    process.env.ERPX_NATIVE_OPERATIONS_ROUTINES_CONFIG = config;
    const data = {...editor, purchases: null};
    transportOverride = vi.fn(async () => reply(data));
    await expect(nativeRoutineEditorContext(editorRequest(), 'acme', 'user-1', transportOverride)).resolves.toEqual(data);
  });

  it('fails closed on a changed context or an incomplete or private wire shape', async () => {
    process.env.ERPX_NATIVE_OPERATIONS_ROUTINES_CONFIG = config;
    transportOverride = vi.fn(async () => reply(editor, {...context, revision: 8}));
    await expect(nativeRoutineEditorContext(editorRequest(), 'acme', 'user-1', transportOverride))
      .rejects.toMatchObject({code: 'NATIVE_ROUTINES_RESPONSE_INVALID'});
    const incomplete: Record<string, unknown> = {...editor, canWrite: undefined};
    transportOverride = vi.fn(async () => reply(incomplete));
    await expect(nativeRoutineEditorContext(editorRequest(), 'acme', 'user-1', transportOverride))
      .rejects.toMatchObject({code: 'NATIVE_ROUTINES_RESPONSE_INVALID'});
    transportOverride = vi.fn(async () => reply({
      ...editor,
      options: {...editor.options, people: [{...editor.options.people[0]!, private_worker_type: 'employee'}]},
    }));
    await expect(nativeRoutineEditorContext(editorRequest(), 'acme', 'user-1', transportOverride))
      .rejects.toMatchObject({code: 'NATIVE_ROUTINES_RESPONSE_INVALID'});
    transportOverride = vi.fn(async () => reply({...editor, purchases: {work: purchase, projects: {newWork: true}, context: 'partial'}}));
    await expect(nativeRoutineEditorContext(editorRequest(), 'acme', 'user-1', transportOverride))
      .rejects.toMatchObject({code: 'NATIVE_ROUTINES_RESPONSE_INVALID'});
  });

  it('fails closed without the native configuration', async () => {
    delete process.env.ERPX_NATIVE_OPERATIONS_ROUTINES_CONFIG;
    const transport = vi.fn();
    transportOverride = transport;
    await expect(nativeRoutineEditorContext(editorRequest(), 'acme', 'user-1', transportOverride))
      .rejects.toMatchObject({code: 'NATIVE_ROUTINES_UNAVAILABLE'});
    expect(transport).not.toHaveBeenCalled();
  });
});

it('keeps the registered Holding Host on a real loopback read', async () => {
  const seen: Array<{host?: string; path?: string}> = [];
  const owner = createServer((incoming, response) => {
    seen.push({host: incoming.headers.host, path: incoming.url});
    response.end(JSON.stringify({data: [row], context}));
  });
  await new Promise<void>((resolveListen, reject) => {
    owner.once('error', reject);
    owner.listen(0, '127.0.0.1', resolveListen);
  });
  try {
    const listener = owner.address();
    if (!listener || typeof listener === 'string') throw Error('Expected a fixture TCP listener');
    vi.stubEnv('ERPX_NATIVE_OPERATIONS_ROUTINES_CONFIG', JSON.stringify({
      privateBaseUrl: `http://127.0.0.1:${listener.port}`, timeoutMs: 1000,
    }));
    await expect(nativeRoutineTemplateList(request(), 'acme', 'user-1')).resolves.toEqual([row]);
    expect(seen).toEqual([{host: 'acme.jesadakorn.com', path: '/api/v1/holdings/acme/routine-templates'}]);
  } finally {
    await new Promise<void>((resolveClose, reject) => owner.close(error => error ? reject(error) : resolveClose()));
  }
});

it('keeps the registered Holding Host on the editor context loopback read', async () => {
  const seen: Array<{host?: string; path?: string}> = [];
  const owner = createServer((incoming, response) => {
    seen.push({host: incoming.headers.host, path: incoming.url});
    response.end(JSON.stringify({data: editor, context}));
  });
  await new Promise<void>((resolveListen, reject) => {
    owner.once('error', reject);
    owner.listen(0, '127.0.0.1', resolveListen);
  });
  try {
    const listener = owner.address();
    if (!listener || typeof listener === 'string') throw Error('Expected a fixture TCP listener');
    vi.stubEnv('ERPX_NATIVE_OPERATIONS_ROUTINES_CONFIG', JSON.stringify({
      privateBaseUrl: `http://127.0.0.1:${listener.port}`, timeoutMs: 1000,
    }));
    await expect(nativeRoutineEditorContext(editorRequest(), 'acme', 'user-1')).resolves.toEqual(editor);
    expect(seen).toEqual([{host: 'acme.jesadakorn.com', path: '/api/v1/holdings/acme/routine-template-editor-context'}]);
  } finally {
    await new Promise<void>((resolveClose, reject) => owner.close(error => error ? reject(error) : resolveClose()));
  }
});
