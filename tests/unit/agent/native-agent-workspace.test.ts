import { describe, expect, it, vi } from 'vitest';

vi.mock('server-only', () => ({}));
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import {
  NativeAgentWorkspaceError,
  createNativeAgentWorkspaceClient,
} from '../../../packages/server-agent/src/public/native-agent-workspace';

const actor: WorkContext = {
  userId: 'user',
  name: 'Owner',
  email: 'owner@example.test',
  assignmentId: 'assignment',
  role: 'holding_owner',
  companyId: 'company',
  branchId: null,
  departmentId: null,
  holdingId: 'holding',
  holdingSlug: 'acme',
  holdingName: 'Acme',
  scope: { kind: 'holding', holdingId: 'holding', holdingSlug: 'acme', holdingName: 'Acme', companyId: 'company' },
  assignmentScope: 'holding',
  authority: { sessionId: 'session', revision: 3, assignmentVersion: 7 },
  inherited: false,
  modules: ['agent'],
  assignments: [],
  workspaces: [],
};

const config = JSON.stringify({
  privateBaseUrl: 'http://127.0.0.1:8199',
  timeoutMs: 1000,
  holdings: [{ origin: 'https://acme.jesadakorn.com', holdingSlug: 'acme', holdingId: 'holding' }],
});

const workspace = {
  agents: [], versions: [], pools: [], assignments: [], invocations: [],
  people: [], members: [], poolHistory: [], sessions: [],
};

type Call = { url: URL; headers: Headers; body: string };

function transport(response: Response, calls: Call[]) {
  return async (url: URL, headers: Headers, body: string): Promise<Response> => {
    calls.push({ url, headers, body });
    return response;
  };
}

const json = (data: unknown, init: ResponseInit = {}) =>
  new Response(JSON.stringify({ data }), {
    status: 200,
    headers: { 'content-type': 'application/json' },
    ...init,
  });

describe('native Agent workspace read client', () => {
  it('dispatches the workspace read to the mounted private route with bounded identity', async () => {
    const calls: Call[] = [];
    const client = createNativeAgentWorkspaceClient(config, transport(json(workspace), calls));
    const incoming = new Headers({ host: 'acme.jesadakorn.com', cookie: 'device=synthetic-test' });
    expect(await client.workspace(actor, false, incoming)).toEqual(workspace);
    expect(calls[0]!.headers.get('cookie')).toBe('device=synthetic-test');
    await expect(client.workspace(actor, false, new Headers({ host: 'northstar.jesadakorn.com' })))
      .rejects.toMatchObject({ status: 403 });
    expect(calls).toHaveLength(1);
    expect(calls[0]!.url.pathname).toBe('/api/v1/holdings/acme/agent/workspace-read');
    expect(calls[0]!.headers.get('host')).toBe('acme.jesadakorn.com');
    expect(calls[0]!.headers.get('x-erpx-context-company')).toBe('company');
    expect(calls[0]!.headers.get('x-erpx-context-revision')).toBe('3');
    expect(JSON.parse(calls[0]!.body)).toEqual({ kind: 'workspace', includeEvidence: false });
  });

  it('dispatches the evidence record read with an allow-listed kind and bounded id', async () => {
    const calls: Call[] = [];
    const record = { row: { id: 'ai-evidence-t', turn_id: 't' }, sessionData: null };
    const client = createNativeAgentWorkspaceClient(config, transport(json(record), calls));
    expect(await client.record(actor, 'agent_invocation', 'ai-evidence-t')).toEqual(record);
    expect(JSON.parse(calls[0]!.body)).toEqual({
      kind: 'record', recordKind: 'agent_invocation', id: 'ai-evidence-t',
    });
    await expect(client.record(actor, 'other' as 'agent_invocation', 'id')).rejects.toThrow(NativeAgentWorkspaceError);
    await expect(client.record(actor, 'agent_session', 'bad\nid')).rejects.toThrow(NativeAgentWorkspaceError);
  });

  it('fails closed on a wrong Holding, non-200 response and malformed payload', async () => {
    const client = createNativeAgentWorkspaceClient(config, transport(json(workspace), []));
    await expect(client.workspace({ ...actor, holdingId: 'other' })).rejects.toThrow(NativeAgentWorkspaceError);

    const failing = createNativeAgentWorkspaceClient(config, transport(
      new Response(JSON.stringify({ error: { code: 'AGENT_WORKSPACE_READ_UNAVAILABLE' } }), {
        status: 503, headers: { 'content-type': 'application/json' },
      }), [],
    ));
    await expect(failing.workspace(actor)).rejects.toThrow(NativeAgentWorkspaceError);

    const malformed = createNativeAgentWorkspaceClient(config, transport(json({ agents: [] }), []));
    await expect(malformed.workspace(actor)).rejects.toThrow(NativeAgentWorkspaceError);
  });

  it('rejects a non-loopback private base URL', () => {
    expect(() => createNativeAgentWorkspaceClient(JSON.stringify({
      privateBaseUrl: 'http://example.com:8199', timeoutMs: 1000,
      holdings: [{ origin: 'https://acme.jesadakorn.com', holdingSlug: 'acme', holdingId: 'holding' }],
    }))).toThrow(NativeAgentWorkspaceError);
  });
});
