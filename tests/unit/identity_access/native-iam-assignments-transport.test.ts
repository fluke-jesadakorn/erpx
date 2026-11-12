import { describe, it, expect, vi, afterEach } from 'vitest';
vi.mock('server-only', () => ({}));
import {
  createNativeIamAssignmentsClient,
  nativeIamAssignmentsAvailable,
  nativeIamAssignmentsConfiguration,
  nativeIamAssignmentsFailure,
  type NativeIamAssignmentsConfiguration,
  type NativeIamPageContext,
} from '../../../packages/server-identity_access/src/public/native-iam-assignments';

const config = (overrides: Record<string, unknown> = {}) =>
  JSON.stringify({
    privateBaseUrl: 'http://127.0.0.1:44993',
    timeoutMs: 1000,
    holdings: [
      { origin: 'https://acme.example.com', holdingSlug: 'acme', holdingId: 'h001' },
    ],
    ...overrides,
  });
const incoming = (overrides: Record<string, string> = {}) =>
  new Headers({
    host: 'acme.example.com',
    cookie: '__Host-erpx-product=session',
    ...overrides,
  });
const actor: NativeIamPageContext = {
  userId: 'actor',
  holdingId: 'h001',
  holdingSlug: 'acme',
  companyId: 'company',
  branchId: null,
  departmentId: null,
  assignmentId: 'assignment-actor',
  role: 'it_admin',
  assignmentScope: 'holding',
  authority: { revision: 9, assignmentVersion: 3 },
  iam: { permissions: ['iam.read', 'iam.manage'] },
};
const context = {
  userId: 'actor',
  holdingId: 'h001',
  companyId: 'company',
  branchId: null,
  departmentId: null,
  assignmentId: 'assignment-actor',
  assignmentVersion: 3,
  revision: 9,
  role: 'it_admin',
  scopeKind: 'holding',
  permissions: ['iam.read', 'iam.manage'],
  expiresAt: 9_007_199_254_740_991,
};
const assignment = {
  id: 'assignment-target',
  user_id: 'target',
  role: 'employee',
  modules_json: '["collaboration"]',
  company_id: 'company',
  branch_id: null,
  valid_from: 0,
  valid_until: 1_000_000,
  revoked_at: null,
  version: 1,
  scope_kind: 'holding',
  scope_node_id: null,
  parent_id: null,
  source: 'manual',
  source_id: null,
  identity_status: 'active',
  login_policy: 'local',
  person_id: null,
  connection_id: null,
  identity_version: 1,
  group_name: null,
  mapping_connection_id: null,
  directory_enabled: null,
  observed_at: null,
  connection_status: null,
  active: true,
};
const role = {
  id: 'employee',
  name: 'Employee',
  companyId: '*',
  revision: 1,
  builtIn: true,
  permissions: ['iam.self'],
  grantablePermissions: [],
  scopes: ['holding', 'branch', 'department'],
  status: 'active',
};
const workspace = {
  assignments: [assignment],
  roles: [role],
  evaluatedAt: 500,
  currentAssignmentId: 'assignment-actor',
  currentUserId: 'actor',
  scopeKind: 'holding',
  scopeNodeId: null,
};
const input = {
  holdingSlug: 'acme',
  expected: {
    companyId: 'company',
    assignmentId: 'assignment-actor',
    revision: 9,
    assignmentVersion: 3,
  },
};
const ok = (value: unknown) =>
  new Response(JSON.stringify(value), {
    status: 200,
    headers: { 'content-type': 'application/json' },
  });
const client = (raw = config(), transport = vi.fn()) =>
  createNativeIamAssignmentsClient(
    nativeIamAssignmentsConfiguration(raw) as NativeIamAssignmentsConfiguration,
    transport,
  );

afterEach(() => {
  delete process.env.ERPX_NATIVE_IDENTITY_ACCESS_CONFIG;
  vi.unstubAllEnvs();
});

describe('native Identity Access assignment transport', () => {
  it('forwards the exact workspace Host, session and context CAS', async () => {
    const transport = vi.fn().mockResolvedValue(ok({ data: workspace, context }));
    const access = client(config(), transport);
    const result = await access.workspace(incoming(), actor, input);
    expect(result.assignments).toEqual([assignment]);
    expect(result.roles).toEqual([role]);
    const [method, url, headers, body] = transport.mock.calls[0];
    expect(method).toBe('GET');
    expect(url.toString()).toBe(
      'http://127.0.0.1:44993/api/v1/holdings/acme/iam/assignments',
    );
    expect(headers.get('host')).toBe('acme.example.com');
    expect(headers.get('cookie')).toBe('__Host-erpx-product=session');
    expect(headers.get('x-erpx-context-company')).toBe('company');
    expect(headers.get('x-erpx-context-assignment')).toBe('assignment-actor');
    expect(headers.get('x-erpx-context-revision')).toBe('9');
    expect(headers.get('x-erpx-context-assignment-version')).toBe('3');
    expect(headers.get('idempotency-key')).toBeNull();
    expect(body).toBeUndefined();
  });

  it('forwards the exact assignment id, body and key for withdrawal', async () => {
    const transport = vi.fn().mockResolvedValue(
      ok({ data: { id: 'assignment-target', status: 'revoked', version: 2 }, context }),
    );
    const access = client(config(), transport);
    const body = JSON.stringify({
      expectedVersion: 1,
      reason: 'Reviewed withdrawal',
    });
    const result = await access.revoke(
      incoming(),
      actor,
      'assignment-target',
      body,
      'iam-revoke-key-1',
      input,
    );
    expect(result).toEqual({ id: 'assignment-target', status: 'revoked', version: 2 });
    const [method, url, headers, sent] = transport.mock.calls[0];
    expect(method).toBe('POST');
    expect(url.toString()).toBe(
      'http://127.0.0.1:44993/api/v1/holdings/acme/iam/assignments/assignment-target/revoke',
    );
    expect(headers.get('idempotency-key')).toBe('iam-revoke-key-1');
    expect(headers.get('content-type')).toBe('application/json');
    expect(JSON.parse(sent!)).toEqual({
      expectedVersion: 1,
      reason: 'Reviewed withdrawal',
    });
  });

  it('surfaces a bounded owner conflict with its status, code and message', async () => {
    const transport = vi.fn().mockResolvedValue(
      new Response(
        JSON.stringify({
          error: {
            code: 'ASSIGNMENT_CHANGED',
            message: 'Reload the current assignment.',
          },
        }),
        { status: 409, headers: { 'content-type': 'application/json' } },
      ),
    );
    const access = client(config(), transport);
    await expect(
      access.revoke(
        incoming(),
        actor,
        'assignment-target',
        JSON.stringify({ expectedVersion: 1, reason: 'Reviewed withdrawal' }),
        'iam-revoke-key-2',
        input,
      ),
    ).rejects.toMatchObject({
      code: 'ASSIGNMENT_CHANGED',
      status: 409,
      message: 'Reload the current assignment.',
    });
  });

  it('rejects foreign Company rows, duplicate rows, extra keys and changed context', async () => {
    const foreign = client(
      config(),
      vi.fn().mockResolvedValue(
        ok({ data: { ...workspace, assignments: [{ ...assignment, company_id: 'foreign' }] }, context }),
      ),
    );
    await expect(foreign.workspace(incoming(), actor, input)).rejects.toMatchObject({
      code: 'NATIVE_IAM_ASSIGNMENTS_RESPONSE_INVALID',
    });
    const duplicate = client(
      config(),
      vi.fn().mockResolvedValue(
        ok({ data: { ...workspace, assignments: [assignment, assignment] }, context }),
      ),
    );
    await expect(duplicate.workspace(incoming(), actor, input)).rejects.toMatchObject({
      code: 'NATIVE_IAM_ASSIGNMENTS_RESPONSE_INVALID',
    });
    const extra = client(
      config(),
      vi.fn().mockResolvedValue(
        ok({ data: { ...workspace, assignments: [{ ...assignment, extra: true }] }, context }),
      ),
    );
    await expect(extra.workspace(incoming(), actor, input)).rejects.toMatchObject({
      code: 'NATIVE_IAM_ASSIGNMENTS_RESPONSE_INVALID',
    });
    const changed = client(
      config(),
      vi.fn().mockResolvedValue(ok({ data: workspace, context: { ...context, revision: 10 } })),
    );
    await expect(changed.workspace(incoming(), actor, input)).rejects.toMatchObject({
      code: 'NATIVE_IAM_ASSIGNMENTS_CONTEXT_CHANGED',
    });
    const foreignHost = client(config(), vi.fn());
    await expect(
      foreignHost.workspace(incoming({ host: 'evil.example.com' }), actor, input),
    ).rejects.toMatchObject({ code: 'NATIVE_IAM_ASSIGNMENTS_AUTHORITY_INVALID' });
    const foreignSlug = client(config(), vi.fn());
    await expect(
      foreignSlug.workspace(incoming(), { ...actor, holdingSlug: 'other' }, input),
    ).rejects.toMatchObject({ code: 'NATIVE_IAM_ASSIGNMENTS_CONTEXT_CHANGED' });
  });

  it('keeps branch assignment rows inside the selected branch', async () => {
    const branchActor = {
      ...actor,
      branchId: 'branch-1',
      assignmentScope: 'branch',
    };
    const branchContext = {
      ...context,
      branchId: 'branch-1',
      scopeKind: 'branch',
    };
    const branchRow = {
      ...assignment,
      branch_id: 'branch-1',
      scope_kind: 'branch',
      scope_node_id: 'branch-1',
    };
    const valid = client(
      config(),
      vi.fn().mockResolvedValue(
        ok({
          data: {
            ...workspace,
            assignments: [branchRow],
            scopeKind: 'branch',
            scopeNodeId: 'branch-1',
          },
          context: branchContext,
        }),
      ),
    );
    await expect(
      valid.workspace(incoming(), branchActor, { holdingSlug: 'acme' }),
    ).resolves.toBeTruthy();
    const foreign = client(
      config(),
      vi.fn().mockResolvedValue(
        ok({ data: { ...workspace, assignments: [assignment] }, context: branchContext }),
      ),
    );
    await expect(
      foreign.workspace(incoming(), branchActor, { holdingSlug: 'acme' }),
    ).rejects.toMatchObject({ code: 'NATIVE_IAM_ASSIGNMENTS_RESPONSE_INVALID' });
  });

  it('fails closed without configuration, on malformed configuration and reserved ports', () => {
    expect(nativeIamAssignmentsAvailable()).toBe(false);
    process.env.ERPX_NATIVE_IDENTITY_ACCESS_CONFIG = config();
    expect(nativeIamAssignmentsAvailable()).toBe(true);
    const codeOf = (run: () => unknown) => {
      try {
        run();
        return undefined;
      } catch (error) {
        return (error as { code?: string }).code;
      }
    };
    expect(codeOf(() => nativeIamAssignmentsConfiguration('{not-json'))).toBe(
      'NATIVE_IAM_ASSIGNMENTS_CONFIGURATION_INVALID',
    );
    for (const raw of [
      config({ privateBaseUrl: 'http://127.0.0.1:3100' }),
      config({ privateBaseUrl: 'http://127.0.0.1:3201' }),
      config({ privateBaseUrl: 'http://localhost:44993' }),
      config({ privateBaseUrl: 'https://127.0.0.1:44993' }),
      config({
        holdings: [
          { origin: 'http://acme.example.com', holdingSlug: 'acme', holdingId: 'h001' },
        ],
      }),
      config({
        holdings: [
          { origin: 'https://acme.example.com', holdingSlug: 'Acme', holdingId: 'h001' },
        ],
      }),
      config({
        holdings: [
          { origin: 'https://acme.example.com', holdingSlug: 'acme', holdingId: 'h001' },
          { origin: 'https://acme.example.com', holdingSlug: 'other', holdingId: 'h002' },
        ],
      }),
    ])
      expect(
        codeOf(() =>
          createNativeIamAssignmentsClient(
            nativeIamAssignmentsConfiguration(raw) as NativeIamAssignmentsConfiguration,
          ),
        ),
      ).toBe('NATIVE_IAM_ASSIGNMENTS_CONFIGURATION_INVALID');
    expect(nativeIamAssignmentsFailure(new Error('other'))).toBeUndefined();
  });

  it('maps owner and configuration failures to the retained page/route policy', async () => {
    const { NativeIamAssignmentsError } = await import(
      '../../../packages/server-identity_access/src/public/native-iam-assignments'
    );
    expect(
      nativeIamAssignmentsFailure(
        new NativeIamAssignmentsError('NATIVE_IAM_ASSIGNMENTS_CONTEXT_CHANGED'),
      ),
    ).toEqual({
      status: 409,
      code: 'WORK_CONTEXT_CHANGED',
      message: 'Your work role changed. Reload and review before retrying.',
    });
    expect(
      nativeIamAssignmentsFailure(
        new NativeIamAssignmentsError('NATIVE_IAM_ASSIGNMENTS_UNAVAILABLE', 503),
      ),
    ).toEqual({
      status: 503,
      code: 'NATIVE_IAM_ASSIGNMENTS_UNAVAILABLE',
      message: 'The native Identity Access service is unavailable.',
    });
    expect(
      nativeIamAssignmentsFailure(
        new NativeIamAssignmentsError('NATIVE_IAM_ASSIGNMENTS_REQUEST_INVALID'),
      ),
    ).toEqual({
      status: 422,
      code: 'IAM_ASSIGNMENT_REQUEST_INVALID',
      message: 'Review the assignment request.',
    });
    expect(
      nativeIamAssignmentsFailure(
        new NativeIamAssignmentsError('NATIVE_IAM_ASSIGNMENTS_CONFIGURATION_INVALID'),
      ),
    ).toEqual({
      status: 503,
      code: 'NATIVE_IAM_ASSIGNMENTS_UNAVAILABLE',
      message: 'The native Identity Access reader is unavailable.',
    });
  });
});

describe('retired access-review command routes', () => {
  const route = () =>
    import(
      '../../../packages/workplace/src/routes/api/v1/holdings/[holdingSlug]/iam/[[...path]]/route'
    );
  afterEach(() => {
    delete process.env.ERPX_NATIVE_IAM_CONFIG;
  });

  it('fails closed for access-review POST instead of falling back to TypeScript', async () => {
    delete process.env.ERPX_NATIVE_IAM_CONFIG;
    const { POST } = await route();
    const response = await POST(
      new Request(
        'https://acme.example.com/api/v1/holdings/acme/iam/access-reviews',
        {
          method: 'POST',
          headers: { 'content-type': 'application/json' },
          body: JSON.stringify({
            name: 'Quarterly access certification',
            dueAt: 1_000_000,
            reason: 'Confirm every active company assignment',
          }),
        },
      ),
      {
        params: Promise.resolve({
          holdingSlug: 'acme',
          path: ['access-reviews'],
        }),
      } as never,
    );
    expect(response.status).toBe(503);
    expect(await response.json()).toMatchObject({
      error: { code: 'NATIVE_IAM_REQUIRED' },
    });
  });

  it('keeps unrelated IAM GET reads on the retained handler', async () => {
    delete process.env.ERPX_NATIVE_IAM_CONFIG;
    const { GET } = await route();
    const response = await GET(
      new Request('https://acme.example.com/api/v1/holdings/acme/iam/access-reviews'),
      {
        params: Promise.resolve({
          holdingSlug: 'acme',
          path: ['access-reviews'],
        }),
      } as never,
    );
    expect(response.status).not.toBe(503);
  });
});
