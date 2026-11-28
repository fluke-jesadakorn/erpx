import { describe, it, expect, vi } from 'vitest';
import { readFile } from 'node:fs/promises';
import { createHash } from 'node:crypto';

vi.mock('server-only', () => ({}));

const adapter = (name: string) =>
  new URL(`../../../modules/manage-erpx/server/src/adapters/${name}`, import.meta.url);
const sha256 = (s: string) => createHash('sha256').update(s, 'utf8').digest('hex');

const OLD_IDENTITY_FROM =
  'FROM (SELECT "company_id", "member_limit", "branch_limit", "ai_monthly_limit", "storage_bytes", "created_at" FROM beta_policy) beta_policy';
const OLD_HOSPITALITY_SELECT =
  'SELECT "company_id","member_limit","branch_limit","ai_monthly_limit","storage_bytes","created_at" FROM beta_policy';
const OLD_INVITATIONS_FROM =
  'FROM (SELECT "id", "email", "branch_id", "invited_by", "token_hash", "expires_at", "accepted_user_id", "accepted_at", "revoked_at", "created_at", "role" FROM beta_team_invitations) i';

describe('manage_erpx beta_policy projection read family', () => {
  it('owns the exact snapshot fragments in the Rust-bound contract only', async () => {
    const identity = await readFile(adapter('identity-snapshot-sources.ts'), 'utf8');
    const contractText = await readFile(adapter('identity-snapshot-sources.json'), 'utf8');
    const rust = await readFile(adapter('identity_snapshot_sources.rs'), 'utf8');
    expect(identity).not.toContain('SELECT');
    expect(identity).toContain('identity-snapshot-sources.json');
    const contract = JSON.parse(contractText) as {
      readonly betaPolicyProjection: string;
      readonly iamApiBetaTeamInvitationsFrom: string;
    };
    expect(contract.betaPolicyProjection).toBe(
      'SELECT "company_id", "member_limit", "branch_limit", "ai_monthly_limit", "storage_bytes", "created_at" FROM beta_policy',
    );
    expect(contract.betaPolicyProjection).toHaveLength(119);
    expect(sha256(contract.betaPolicyProjection)).toBe(
      'de44919b37985915cbf1258229cb913bab6f7a7de5ca0010a28d09f078c5e3e6',
    );
    expect(contract.iamApiBetaTeamInvitationsFrom).toBe(OLD_INVITATIONS_FROM);
    expect(contract.iamApiBetaTeamInvitationsFrom).toHaveLength(182);
    expect(sha256(contract.iamApiBetaTeamInvitationsFrom)).toBe(
      '955aa500771bff8ce13f6582bccd9bf615858d2dc32596f8b52b8d27a7576c35',
    );
    expect(rust).toContain('include_str!("identity-snapshot-sources.json")');
    expect(rust).toContain('pub fn beta_policy_projection');
    expect(rust).toContain('pub fn beta_team_invitations_from');
  });

  it('keeps the hospitality surface reusing the one canonical declaration', async () => {
    const hospitality = await readFile(adapter('hospitality-owner-projection-sources.ts'), 'utf8');
    expect(hospitality).not.toMatch(/SELECT "company_id"/);
    expect(hospitality).toContain('import { betaPolicyProjection } from "./identity-snapshot-sources"');
    expect(hospitality).toContain('betaPolicyProjection.replaceAll(", ", ",")');
  });

  it('preserves the exact query text consumed by both existing callers', async () => {
    const { betaPolicyProjection, identitySnapshotSources } = await import(
      '../../../modules/manage-erpx/server/src/adapters/identity-snapshot-sources'
    );
    const { hospitalityOwnerProjectionSources } = await import(
      '../../../modules/manage-erpx/server/src/adapters/hospitality-owner-projection-sources'
    );
    expect(identitySnapshotSources().beta_access_beta_policy_from).toBe(OLD_IDENTITY_FROM);
    expect(identitySnapshotSources().iam_api_beta_team_invitations_i_from).toBe(OLD_INVITATIONS_FROM);
    expect(hospitalityOwnerProjectionSources.beta_policy).toBe(OLD_HOSPITALITY_SELECT);
    // Both surfaces trace back to the one canonical projection declaration.
    expect(identitySnapshotSources().beta_access_beta_policy_from).toContain(betaPolicyProjection);
  });
});

const OLD_BOOTSTRAP_SCOPE =
  'SELECT 1 FROM beta_bootstrap h WHERE h.singleton=1 AND h.holding_id=?';
const OLD_BOOTSTRAP_FENCE = `EXISTS(${OLD_BOOTSTRAP_SCOPE})`;

describe('manage_erpx ai-gateway beta_bootstrap scope read family', () => {
  it('keeps one parameterized beta_bootstrap head shared by both callers', async () => {
    const src = await readFile(adapter('ai-gateway-sources.ts'), 'utf8');
    const helper = await readFile(adapter('beta-bootstrap-point-sql.ts'), 'utf8');
    // The IAM gateway fence and the private-loopback scope read reuse one
    // canonical parameterized head so their holding predicates cannot drift.
    expect(src).not.toMatch(/SELECT 1 FROM beta_bootstrap h WHERE h\.singleton=1 AND h\.holding_id=\?/);
    expect(src).toContain("betaBootstrapPointSql('1', 'h', ' AND h.holding_id=?')");
    expect(helper.match(/SELECT \$\{projection\} FROM beta_bootstrap/g)).toHaveLength(1);
  });

  it('preserves the exact query text consumed by both existing callers', async () => {
    const { betaBootstrapScope, aiGatewayBootstrapSources } = await import(
      '../../../modules/manage-erpx/server/src/adapters/ai-gateway-sources'
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

const OLD_STORAGE_BYTES_SQL = 'SELECT storage_bytes FROM beta_policy WHERE company_id=?';

describe('manage_erpx beta_policy storage-bytes read pair', () => {
  it('keeps one fixed-SQL storage-bytes head shared by both callers', async () => {
    const helper = await readFile(adapter('beta-policy-limit-sql.ts'), 'utf8');
    const attachments = await readFile(adapter('attachments-service-facts.ts'), 'utf8');
    const allowance = await readFile(adapter('legal-portal-storage-allowance.ts'), 'utf8');
    // One canonical head; the quota, doc-page and portal readers reuse it by
    // reference so their prepared SQL cannot drift.
    expect(helper.match(/SELECT \$\{projection\} FROM beta_policy WHERE company_id=\?/g)).toHaveLength(1);
    expect(attachments).not.toMatch(/SELECT storage_bytes FROM beta_policy WHERE company_id=\?/);
    expect(attachments).toContain("betaPolicyLimitSql('storage_bytes')");
    expect(allowance).not.toMatch(/SELECT storage_bytes FROM beta_policy WHERE company_id=\?/);
    expect(allowance).toContain('import {betaPolicyStorageBytesSql} from "./attachments-service-facts"');
  });

  it('preserves the exact prepared SQL for both existing callers', async () => {
    const { betaPolicyStorageBytesSql, attachmentServiceManageErpxFacts } = await import(
      '../../../modules/manage-erpx/server/src/adapters/attachments-service-facts'
    );
    const { legalPortalStorageAllowance } = await import(
      '../../../modules/manage-erpx/server/src/adapters/legal-portal-storage-allowance'
    );
    expect(betaPolicyStorageBytesSql).toBe(OLD_STORAGE_BYTES_SQL);
    const prepared: string[] = [];
    const db = {
      prepare(sql: string) {
        prepared.push(sql);
        return {
          bind() { return { first: async () => ({ storage_bytes: 1 }) }; },
          first: async () => ({ storage_bytes: 1 }),
        };
      },
    };
    await attachmentServiceManageErpxFacts(db as never).readBetaPolicy20({ companyId: 'c1' });
    await legalPortalStorageAllowance(db as never, 'c1');
    expect(prepared).toEqual([OLD_STORAGE_BYTES_SQL, OLD_STORAGE_BYTES_SQL]);
  });
});

const OLD_DOCUMENT_PAGE_STORAGE_LIMIT =
  'COALESCE((SELECT storage_bytes FROM beta_policy WHERE company_id=?),9007199254740991)';

describe('manage_erpx document page storage-limit projection dedupe', () => {
  it('embeds the one canonical storage-bytes read instead of repeating its SQL', async () => {
    const src = await readFile(adapter('document-page-storage-limit.ts'), 'utf8');
    expect(src).not.toMatch(/SELECT storage_bytes FROM beta_policy WHERE company_id=\?/);
    expect(src).toContain("import {betaPolicyStorageBytesSql} from './attachments-service-facts'");
  });

  it('preserves the exact SQL consumed by the server-files attachment derivative guard', async () => {
    const { documentPageStorageLimitSql } = await import(
      '../../../modules/manage-erpx/server/src/adapters/document-page-storage-limit'
    );
    expect(documentPageStorageLimitSql).toBe(OLD_DOCUMENT_PAGE_STORAGE_LIMIT);
  });
});

describe('manage_erpx beta_policy point-read parameterized family', () => {
  it('keeps the retired point-read bytes after whitespace normalization', async () => {
    const { betaPolicyPointSql } = await import(
      '../../../modules/manage-erpx/server/src/adapters/beta-policy-point-sql'
    );
    const canonical = (value: string) => value.replace(/\s+/g, ' ');
    expect(canonical(betaPolicyPointSql('*'))).toBe(
      'SELECT * FROM beta_policy WHERE company_id=?',
    );
    expect(canonical(betaPolicyPointSql('member_limit'))).toBe(
      'SELECT member_limit FROM beta_policy WHERE company_id=?',
    );
    expect(canonical(betaPolicyPointSql('branch_limit'))).toBe(
      'SELECT branch_limit FROM beta_policy WHERE company_id=?',
    );
    expect(betaPolicyPointSql('*').match(/\?/g)).toHaveLength(1);
  });

  it('delegates the standalone reader to the native kinds and keeps composed guards on the authority text', async () => {
    const helper = await readFile(adapter('beta-policy-point-sql.ts'), 'utf8');
    expect(helper).not.toContain('SELECT ');
    expect(helper).toContain('betaPolicyPointKind');
    const api = await readFile(adapter('beta-api.ts'), 'utf8');
    expect(api).toContain('nativeManageOwnerReadFirst(betaPolicyPointKind[\'*\']');
    expect(api).not.toContain('db\n              .prepare(betaPolicyPointSql(\'*\'))');
    const team = await readFile(adapter('beta-team.ts'), 'utf8');
    expect(team).toContain('${betaPolicyPointSql(\'member_limit\')}');
    expect(team).toContain('${betaPolicyPointSql(\'branch_limit\')}');
    expect(team).not.toMatch(
      /SELECT (?:\*|member_limit|branch_limit) FROM beta_policy WHERE company_id=\?/,
    );
  });
});
