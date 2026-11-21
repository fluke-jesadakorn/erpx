import { describe, expect, it } from 'vitest';
import {
  createNativeAgentEvidenceClient,
  nativeAgentEvidenceConfiguration,
  nativeAgentEvidenceFailure,
  NativeAgentEvidenceError,
  type NativeAgentEvidenceConfiguration,
  type NativeAgentInvocation,
  type NativeAgentPageContext,
  type NativeAgentSession,
  type NativeAgentEvidenceTransport,
} from '@erpx/server-agent/public/native-agent-evidence';

const digest = 'a'.repeat(64);
const actor: NativeAgentPageContext = {
  userId: 'claimant',
  holdingId: 'h001',
  holdingSlug: 'acme',
  companyId: 'company',
  branchId: null,
  departmentId: null,
  branchSlug: null,
  departmentSlug: null,
  assignmentId: 'assignment',
  role: 'employee',
  assignmentScope: 'holding',
  authority: { revision: 5, assignmentVersion: 2 },
  modules: ['agent'],
  iam: { permissions: ['records.read'] },
};

const configuration: NativeAgentEvidenceConfiguration = {
  privateBaseUrl: 'http://127.0.0.1:3111',
  timeoutMs: 1000,
  holdings: [{ origin: 'https://acme.example.test', holdingSlug: 'acme', holdingId: 'h001' }],
};

function context(overrides: Record<string, unknown> = {}) {
  return {
    userId: actor.userId,
    holdingId: actor.holdingId,
    companyId: actor.companyId,
    branchId: actor.branchId,
    departmentId: actor.departmentId,
    branchSlug: actor.branchSlug,
    departmentSlug: actor.departmentSlug,
    assignmentId: actor.assignmentId,
    assignmentVersion: actor.authority.assignmentVersion,
    revision: actor.authority.revision,
    role: actor.role,
    assignmentScopeKind: actor.assignmentScope,
    modules: [...actor.modules],
    permissions: [...actor.iam.permissions],
    ...overrides,
  };
}

function invocation(overrides: Partial<NativeAgentInvocation> = {}): NativeAgentInvocation {
  return {
    id: 'i1',
    session_id: 's1',
    turn_id: 'turn-1',
    company_id: 'company',
    user_id: 'claimant',
    agent_version_id: null,
    question_digest: digest,
    sources_digest: digest,
    outcome: 'succeeded',
    input_tokens: 10,
    output_tokens: 20,
    error: null,
    created_at: 10,
    ...overrides,
  };
}

function session(overrides: Partial<NativeAgentSession> = {}): NativeAgentSession {
  return {
    id: 's1',
    company_id: 'company',
    user_id: 'claimant',
    assignment_id: 'assignment',
    conversation_key: 'conversation-1',
    generation: 1,
    created_at: 1,
    status: 'active',
    expires_at: 2000,
    legal_hold: false,
    hold_reason: null,
    version: 1,
    last_used_at: 1,
    updated_at: 10,
    effective_status: 'active',
    ...overrides,
  };
}

function payload(overrides: Record<string, unknown> = {}) {
  return { now: 1000, invocations: [invocation()], sessions: [session()], ...overrides };
}

function response(data: unknown, status = 200) {
  return new Response(JSON.stringify({ data, context: context() }), {
    status,
    headers: { 'content-type': 'application/json' },
  });
}

function client(data: unknown = payload()) {
  const seen: Array<{ method: string; url: URL; headers: Headers }> = [];
  const transport: NativeAgentEvidenceTransport = async (method, url, headers) => {
    seen.push({ method, url, headers });
    return response(data);
  };
  return { seen, client: createNativeAgentEvidenceClient(configuration, transport) };
}

const incoming = (host = 'acme.example.test') => ({
  get: (name: string) => {
    if (name === 'host') return host;
    if (name === 'cookie') return 'erpx_session=abc';
    return null;
  },
});

describe('native Agent evidence transport', () => {
  it('forwards the registered host, cookie and current context and returns the bounded workspace', async () => {
    const { seen, client: instance } = client();
    const result = await instance.evidence(incoming(), actor);
    expect(result.now).toBe(1000);
    expect(result.invocations.map((row) => row.id)).toEqual(['i1']);
    expect(result.sessions[0].effective_status).toBe('active');
    expect(seen).toHaveLength(1);
    expect(seen[0].method).toBe('GET');
    expect(seen[0].url.pathname).toBe('/api/v1/holdings/acme/agents/evidence');
    expect(seen[0].headers.get('host')).toBe('acme.example.test');
    expect(seen[0].headers.get('cookie')).toBe('erpx_session=abc');
    expect(seen[0].headers.get('x-erpx-context-company')).toBe('company');
    expect(seen[0].headers.get('x-erpx-context-assignment')).toBe('assignment');
    expect(seen[0].headers.get('x-erpx-context-revision')).toBe('5');
    expect(seen[0].headers.get('x-erpx-context-assignment-version')).toBe('2');
  });

  it('computes the effective expiry from the returned server time', async () => {
    const { client: instance } = client(
      payload({
        sessions: [
          session({ id: 's2', expires_at: 1000, updated_at: 20, effective_status: 'expired' }),
          session({ id: 's1', expires_at: 2000, updated_at: 10 }),
        ],
      }),
    );
    const result = await instance.evidence(incoming(), actor);
    expect(result.sessions.map((row) => row.effective_status)).toEqual(['expired', 'active']);
  });

  it('rejects a foreign host, a mismatched Holding and a changed context', async () => {
    const { client: instance } = client();
    await expect(instance.evidence(incoming('other.example.test'), actor)).rejects.toMatchObject({
      code: 'NATIVE_AGENT_AUTHORITY_INVALID',
    });
    await expect(
      instance.evidence(incoming(), { ...actor, holdingId: 'h002' }),
    ).rejects.toMatchObject({ code: 'NATIVE_AGENT_AUTHORITY_INVALID' });
    const changed = createNativeAgentEvidenceClient(
      configuration,
      async () =>
        new Response(
          JSON.stringify({ data: payload(), context: context({ revision: 6 }) }),
          { status: 200, headers: { 'content-type': 'application/json' } },
        ),
    );
    await expect(changed.evidence(incoming(), actor)).rejects.toMatchObject({
      code: 'NATIVE_AGENT_CONTEXT_CHANGED',
    });
  });

  it('fails closed on a foreign Company, a non-manager foreign session and a bad effective status', async () => {
    for (const data of [
      payload({ invocations: [invocation({ company_id: 'foreign' })] }),
      payload({ sessions: [session({ user_id: 'other' })] }),
      payload({ sessions: [session({ expires_at: 1000, effective_status: 'active' })] }),
      payload({ invocations: [invocation({ question_digest: 'A'.repeat(64) })] }),
      payload({ invocations: [{ ...invocation(), outcome: 'unknown' }] }),
      payload({ sessions: [session({ legal_hold: true, hold_reason: null })] }),
      payload({ sessions: [session({ generation: 0 })] }),
    ]) {
      const { client: instance } = client(data);
      await expect(instance.evidence(incoming(), actor)).rejects.toMatchObject({
        code: 'NATIVE_AGENT_RESPONSE_INVALID',
      });
    }
  });

  it('rejects duplicates, out-of-order rows and oversized collections', async () => {
    for (const data of [
      payload({ invocations: [invocation(), invocation()] }),
      payload({ sessions: [session(), session()] }),
      payload({
        invocations: [invocation({ id: 'i1', created_at: 10 }), invocation({ id: 'i2', created_at: 20 })],
      }),
      payload({
        invocations: Array.from({ length: 51 }, (_, index) =>
          invocation({ id: `i${index}`, created_at: 100 - index }),
        ),
      }),
      payload({
        sessions: Array.from({ length: 101 }, (_, index) =>
          session({ id: `s${index}`, updated_at: 200 - index }),
        ),
      }),
    ]) {
      const { client: instance } = client(data);
      await expect(instance.evidence(incoming(), actor)).rejects.toMatchObject({
        code: 'NATIVE_AGENT_RESPONSE_INVALID',
      });
    }
  });

  it('rejects response envelopes with extra keys or a non-JSON body', async () => {
    const extra = createNativeAgentEvidenceClient(
      configuration,
      async () =>
        new Response(JSON.stringify({ data: payload(), context: context(), extra: true }), {
          status: 200,
          headers: { 'content-type': 'application/json' },
        }),
    );
    await expect(extra.evidence(incoming(), actor)).rejects.toMatchObject({
      code: 'NATIVE_AGENT_RESPONSE_INVALID',
    });
    const malformed = createNativeAgentEvidenceClient(
      configuration,
      async () =>
        new Response('{}', { status: 200, headers: { 'content-type': 'text/plain' } }),
    );
    await expect(malformed.evidence(incoming(), actor)).rejects.toMatchObject({
      code: 'NATIVE_AGENT_RESPONSE_INVALID',
    });
  });

  it('maps owner failures and network failures without a TypeScript fallback', async () => {
    for (const [status, code, expected] of [
      [401, 'UNAUTHENTICATED', 401],
      [403, 'PERMISSION_REQUIRED', 403],
      [404, 'AGENT_RECORD_NOT_FOUND', 404],
      [409, 'WORK_CONTEXT_CHANGED', 409],
      [422, 'AGENT_EVIDENCE_REQUEST_INVALID', 422],
      [503, 'NATIVE_AGENT_UNAVAILABLE', 503],
    ] as const) {
      const failing = createNativeAgentEvidenceClient(configuration, async () =>
        new Response(JSON.stringify({ error: { code, message: 'Bounded failure.' } }), {
          status,
          headers: { 'content-type': 'application/json' },
        }),
      );
      await expect(failing.evidence(incoming(), actor)).rejects.toMatchObject({ status });
      const failure = nativeAgentEvidenceFailure(
        await failing.evidence(incoming(), actor).catch((error: unknown) => error),
      );
      expect(failure?.status).toBe(expected);
    }
    const network = createNativeAgentEvidenceClient(configuration, async () => {
      throw new Error('connection refused');
    });
    await expect(network.evidence(incoming(), actor)).rejects.toMatchObject({
      code: 'NATIVE_AGENT_UNAVAILABLE',
    });
  });

  it('maps context and configuration failures to retained status codes', () => {
    expect(nativeAgentEvidenceFailure(new NativeAgentEvidenceError('NATIVE_AGENT_CONTEXT_CHANGED'))).toEqual({
      status: 409,
      code: 'WORK_CONTEXT_CHANGED',
      message: 'Your work role changed. Reload and review before continuing.',
    });
    expect(nativeAgentEvidenceFailure(new NativeAgentEvidenceError('NATIVE_AGENT_CONFIGURATION_INVALID'))).toEqual({
      status: 503,
      code: 'NATIVE_AGENT_UNAVAILABLE',
      message: 'The native Agent evidence is unavailable.',
    });
    expect(nativeAgentEvidenceFailure(new Error('other'))).toBeUndefined();
  });
});

describe('native Agent evidence configuration', () => {
  it('accepts the exact loopback configuration', () => {
    expect(
      nativeAgentEvidenceConfiguration(
        JSON.stringify({
          privateBaseUrl: 'http://127.0.0.1:3111',
          timeoutMs: 1000,
          holdings: [{ origin: 'https://acme.example.test', holdingSlug: 'acme', holdingId: 'h001' }],
        }),
      ),
    ).toEqual(configuration);
  });

  it('rejects malformed, reserved, non-loopback and duplicate targets', () => {
    const base = {
      privateBaseUrl: 'http://127.0.0.1:3111',
      timeoutMs: 1000,
      holdings: [{ origin: 'https://acme.example.test', holdingSlug: 'acme', holdingId: 'h001' }],
    };
    for (const value of [
      'not-json',
      JSON.stringify({ ...base, extra: true }),
      JSON.stringify({ ...base, timeoutMs: 249 }),
      JSON.stringify({ ...base, timeoutMs: 30_001 }),
      JSON.stringify({ ...base, privateBaseUrl: 'http://127.0.0.1:3100' }),
      JSON.stringify({ ...base, privateBaseUrl: 'http://127.0.0.1:3200' }),
      JSON.stringify({ ...base, privateBaseUrl: 'http://192.168.1.10:3111' }),
      JSON.stringify({ ...base, privateBaseUrl: 'https://127.0.0.1:3111' }),
      JSON.stringify({
        ...base,
        holdings: [{ origin: 'http://acme.example.test', holdingSlug: 'acme', holdingId: 'h001' }],
      }),
      JSON.stringify({
        ...base,
        holdings: [
          { origin: 'https://acme.example.test', holdingSlug: 'acme', holdingId: 'h001' },
          { origin: 'https://acme.example.test', holdingSlug: 'other', holdingId: 'h002' },
        ],
      }),
      JSON.stringify({
        ...base,
        holdings: [{ origin: 'https://acme.example.test', holdingSlug: 'Bad Slug', holdingId: 'h001' }],
      }),
    ])
      expect(() => nativeAgentEvidenceConfiguration(value)).toThrow(NativeAgentEvidenceError);
  });
});
