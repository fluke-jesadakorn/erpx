import { describe, it, expect, vi } from 'vitest';
import { readFile } from 'node:fs/promises';

vi.mock('server-only', () => ({}));

const adapter = (name: string) =>
  new URL(`../../../packages/server-manage_erpx/src/adapters/${name}`, import.meta.url);

const OLD_IDENTITY_FROM =
  'FROM (SELECT "company_id", "member_limit", "branch_limit", "ai_monthly_limit", "storage_bytes", "created_at" FROM beta_policy) beta_policy';
const OLD_HOSPITALITY_SELECT =
  'SELECT "company_id","member_limit","branch_limit","ai_monthly_limit","storage_bytes","created_at" FROM beta_policy';

describe('manage_erpx beta_policy projection read family', () => {
  it('keeps one fixed-SQL beta_policy projection literal shared by both owner surfaces', async () => {
    const identity = await readFile(adapter('identity-snapshot-sources.ts'), 'utf8');
    const hospitality = await readFile(adapter('hospitality-owner-projection-sources.ts'), 'utf8');
    // One canonical declaration; the other surface reuses it by reference so
    // the IAM snapshot and hospitality/Files/Runtime callers cannot drift.
    expect(identity.match(/SELECT "company_id"/g)).toHaveLength(1);
    expect(hospitality).not.toMatch(/SELECT "company_id"/);
    expect(hospitality).toContain('import { betaPolicyProjection } from "./identity-snapshot-sources"');
    expect(hospitality).toContain('betaPolicyProjection.replaceAll(", ", ",")');
  });

  it('preserves the exact query text consumed by both existing callers', async () => {
    const { betaPolicyProjection, identitySnapshotSources } = await import(
      '../../../packages/server-manage_erpx/src/adapters/identity-snapshot-sources'
    );
    const { hospitalityOwnerProjectionSources } = await import(
      '../../../packages/server-manage_erpx/src/adapters/hospitality-owner-projection-sources'
    );
    expect(identitySnapshotSources().beta_access_beta_policy_from).toBe(OLD_IDENTITY_FROM);
    expect(hospitalityOwnerProjectionSources.beta_policy).toBe(OLD_HOSPITALITY_SELECT);
    // Both surfaces trace back to the one canonical projection declaration.
    expect(identitySnapshotSources().beta_access_beta_policy_from).toContain(betaPolicyProjection);
  });
});

const OLD_BOOTSTRAP_SCOPE =
  'SELECT 1 FROM beta_bootstrap h WHERE h.singleton=1 AND h.holding_id=?';
const OLD_BOOTSTRAP_FENCE = `EXISTS(${OLD_BOOTSTRAP_SCOPE})`;

describe('manage_erpx ai-gateway beta_bootstrap scope read family', () => {
  it('keeps one fixed-SQL beta_bootstrap scope literal shared by both callers', async () => {
    const src = await readFile(adapter('ai-gateway-sources.ts'), 'utf8');
    // The IAM gateway fence and the private-loopback scope read reuse one
    // canonical declaration so their holding predicates cannot drift.
    expect(
      src.match(/SELECT 1 FROM beta_bootstrap h WHERE h\.singleton=1 AND h\.holding_id=\?/g),
    ).toHaveLength(1);
  });

  it('preserves the exact query text consumed by both existing callers', async () => {
    const { betaBootstrapScope, aiGatewayBootstrapSources } = await import(
      '../../../packages/server-manage_erpx/src/adapters/ai-gateway-sources'
    );
    expect(betaBootstrapScope).toBe(OLD_BOOTSTRAP_SCOPE);
    let prepared = '';
    const db = {
      prepare(sql: string) {
        prepared = sql;
        return {
          bind() { return { first: async () => ({ ok: 1 }) }; },
          first: async () => ({ ok: 1 }),
        };
      },
    };
    const sources = aiGatewayBootstrapSources(db as never);
    expect(sources.scopeFence).toBe(OLD_BOOTSTRAP_FENCE);
    expect(sources.statementSource).toBe(
      'FROM beta_bootstrap h WHERE h.singleton=1 AND h.holding_id=?',
    );
    expect(await sources.scope('h001')).toBe(true);
    expect(prepared).toBe(OLD_BOOTSTRAP_SCOPE);
  });
});
