import { afterEach, beforeEach, expect, it, vi } from 'vitest';
vi.mock('server-only', () => ({}));

import {
  nativeEffectiveGpoRules,
  nativeGpoRules,
  nativeGpoRulesOverview,
  nativeMarkPositionGpoNotRequired,
  nativePositionGpoCommand,
  nativePositionGpoOverview,
  nativePreviewGpoRule,
  nativePublishGpoRule,
  nativeRemoveGpoRule,
  nativeRetryGpoRuleConfig,
} from '@erpx/identity-access-server/public/native-position-gpo';

/**
 * Native Position GPO transport oracle. The owner's command, transaction and
 * Directory evidence assertions live in the Rust owner suites
 * (`rust-position-gpo`, `rust-position-gpo-postgres`, `rust-position-gpo-http`);
 * this suite holds the exact Next wire contract after the gpo-rules cutover:
 * registered Holding target, context forwarding, idempotency key, payload
 * translation and the projected read ports.
 */
const CONFIGURATION = JSON.stringify({
  privateBaseUrl: 'http://127.0.0.1:3311',
  timeoutMs: 5_000,
  holdings: [
    { origin: 'https://acme.jesadakorn.com', holdingSlug: 'acme', holdingId: 'H001' },
  ],
});

const request = () => new Request('https://acme.jesadakorn.com/api/v1/acme/technology/gpo-rules', {
  headers: {
    host: 'acme.jesadakorn.com',
    origin: 'https://acme.jesadakorn.com',
    cookie: 'erpx_session=secret',
    'x-erpx-context-company': 'acme',
    'x-erpx-workspace-branch': 'branch-1',
  },
});

const overview = {
  rules: [{
    id: 'rule-1', target: { kind: 'position', id: 'position-1' }, targetName: 'Head',
    version: 1, revision: 2, connectionId: 'conn-1', gpoId: 'gpo-1',
    userFilterGroupId: 'group-1', computerTargetOuId: 'ou-1', active: true,
    directoryStatus: 'directory_applied', directoryError: null, legacyPositionId: null,
  }],
  effectiveByPosition: {
    'position-1': [{
      id: 'rule-1', target: { kind: 'position', id: 'position-1' }, targetName: 'Head',
      version: 1, revision: 2, connectionId: 'conn-1', gpoId: 'gpo-1',
      userFilterGroupId: 'group-1', computerTargetOuId: 'ou-1', active: true,
      directoryStatus: 'directory_applied', directoryError: null, legacyPositionId: null,
      inherited: false,
    }],
  },
  jobs: [],
  tasks: [{ positionId: 'position-1', positionName: 'Head', version: 1, status: 'completed', reason: null, mappingCount: 1, verifiedCount: 1 }],
  canManage: true,
};

const preview = {
  token: 'a'.repeat(64),
  position: { id: 'position-1', title: 'Head' },
  version: 1,
  revision: 2,
  holders: [{ person_id: 'person-1', engagement_id: 'engagement-1', name: 'Ada', account_id: 'account-1' }],
  changes: { membersToAdd: ['account-1'], membersToRemoveFromPreviousGroup: [], directoryConfiguration: true },
  sharedMappings: [],
  endpointStatus: 'unconfirmed',
};

let calls: { url: string; method: string; headers: Headers; body?: unknown }[];

beforeEach(() => {
  process.env.ERPX_NATIVE_IDENTITY_ACCESS_CONFIG = CONFIGURATION;
  calls = [];
  vi.stubGlobal('fetch', async (url: string, init: RequestInit) => {
    calls.push({
      url,
      method: String(init.method),
      headers: new Headers(init.headers),
      body: init.body === undefined ? undefined : JSON.parse(String(init.body)),
    });
    return new Response(JSON.stringify({ data: calls[0].url.includes('/preview') ? preview : overview }), {
      status: 200,
      headers: { 'content-type': 'application/json' },
    });
  });
});

afterEach(() => {
  vi.unstubAllGlobals();
  delete process.env.ERPX_NATIVE_IDENTITY_ACCESS_CONFIG;
});

it('reads the native overview over the private Holding target', async () => {
  const data = await nativeGpoRulesOverview(request(), 'acme');
  expect(data).toEqual(overview);
  expect(calls).toHaveLength(1);
  expect(calls[0].url).toBe('http://127.0.0.1:3311/api/v1/holdings/acme/technology/position-gpo');
  expect(calls[0].method).toBe('GET');
  expect(calls[0].headers.get('host')).toBe('acme.jesadakorn.com');
  expect(calls[0].headers.get('cookie')).toBe('erpx_session=secret');
  expect(calls[0].headers.get('x-erpx-workspace-branch')).toBe('branch-1');
});

it('forwards the not-required command with its idempotency key', async () => {
  const body = { positionId: 'position-1', expectedVersion: 1, reason: 'No Windows policy needed' };
  await nativeMarkPositionGpoNotRequired(request(), 'acme', body, 'key-not-required');
  expect(calls[0].url).toBe('http://127.0.0.1:3311/api/v1/holdings/acme/technology/position-gpo/not-required');
  expect(calls[0].headers.get('idempotency-key')).toBe('key-not-required');
  expect(calls[0].body).toEqual(body);
});

it('rejects an unmounted command before any transport call', async () => {
  await expect(nativePositionGpoCommand(request(), 'acme', 'unknown', {}, 'key')).rejects.toMatchObject({ code: 'NOT_FOUND' });
  expect(calls).toHaveLength(0);
});

it('translates the gpo-rules preview payload and projects its reviewed result', async () => {
  const input = {
    target: { kind: 'position', id: 'position-1' }, expectedVersion: 0,
    connectionId: 'conn-1', gpoId: 'gpo-1', userFilterGroupId: 'group-1',
    computerTargetOuId: 'ou-1', reason: 'Assign inherited organization policy',
  };
  const result = await nativePreviewGpoRule(request(), 'acme', input);
  expect(calls[0].url).toBe('http://127.0.0.1:3311/api/v1/holdings/acme/technology/position-gpo/preview');
  expect(calls[0].body).toEqual({
    positionId: 'position-1', expectedVersion: 0, connectionId: 'conn-1', gpoId: 'gpo-1',
    userFilterGroupId: 'group-1', computerTargetOuId: 'ou-1', reason: input.reason,
  });
  expect(result).toMatchObject({
    token: preview.token,
    target: { kind: 'position', id: 'position-1' },
    targetName: 'Head',
    positions: [{ id: 'position-1', title: 'Head' }],
    holders: [{ personId: 'person-1', engagementId: 'engagement-1', name: 'Ada', accountIds: ['account-1'] }],
    changes: { add: ['account-1'], remove: [], retained: [] },
    inheritedByFuturePositions: false,
    endpointStatus: 'unconfirmed',
  });
});

it('forwards the reviewed token and key on publish', async () => {
  await nativePublishGpoRule(request(), 'acme', {
    target: { kind: 'position', id: 'position-1' }, expectedVersion: 0,
    connectionId: 'conn-1', gpoId: 'gpo-1', userFilterGroupId: 'group-1',
    computerTargetOuId: 'ou-1', reason: 'Assign reviewed policy', previewToken: 'a'.repeat(64),
  }, 'key-publish');
  expect(calls[0].url).toBe('http://127.0.0.1:3311/api/v1/holdings/acme/technology/position-gpo/publish');
  expect(calls[0].headers.get('idempotency-key')).toBe('key-publish');
  expect(calls[0].body).toMatchObject({ positionId: 'position-1', previewToken: 'a'.repeat(64) });
});

it('resolves a rule id to its Position for remove and config retry', async () => {
  await nativeRemoveGpoRule(request(), 'acme', { id: 'rule-1', expectedVersion: 1, reason: 'Remove direct rule', previewToken: 'a'.repeat(64) }, 'key-remove');
  expect(calls.map(call => call.url.split('/').pop())).toEqual(['position-gpo', 'unmap']);
  expect(calls[1].body).toMatchObject({ positionId: 'position-1', expectedVersion: 1, previewToken: 'a'.repeat(64) });
  calls = [];
  await nativeRetryGpoRuleConfig(request(), 'acme', { id: 'rule-1', expectedVersion: 1, reason: 'Retry config' }, 'key-retry');
  expect(calls[1].body).toEqual({ positionId: 'position-1', expectedVersion: 1, reason: 'Retry config' });
});

it('projects the native overview into the owner read ports', async () => {
  expect(await nativeGpoRules(new Headers({ host: 'acme.jesadakorn.com' }), 'H001', 'acme')).toMatchObject([
    { id: 'rule-1', company_id: 'acme', target_kind: 'position', target_id: 'position-1', directory_config_status: 'directory_applied' },
  ]);
  expect(await nativeEffectiveGpoRules(new Headers({ host: 'acme.jesadakorn.com' }), 'H001', 'acme')).toMatchObject([
    { id: 'rule-1', company_id: 'acme', position_id: 'position-1' },
  ]);
});

it('maps native failures into the existing error envelope', async () => {
  vi.stubGlobal('fetch', async () => new Response(JSON.stringify({ error: { code: 'GPO_TASK_CHANGED', message: 'Reload the task' } }), {
    status: 409, headers: { 'content-type': 'application/json' },
  }));
  await expect(nativePositionGpoOverview(request(), 'acme')).rejects.toMatchObject({ code: 'GPO_TASK_CHANGED', status: 409 });
});
