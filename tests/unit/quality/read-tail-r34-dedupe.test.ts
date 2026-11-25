import { describe, expect, test, vi } from 'vitest';
import { readdirSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { createHash } from 'node:crypto';
vi.mock('server-only', () => ({}));
import { commercialOwnerProjectionSources as identityAccessProjectionSources } from '@erpx/server-identity_access/public/commercial-owner-projection-sources';
import { evidenceHead } from '../../../packages/server-quality/src/adapters/inspection-evidence-source-sql';
import { qualityAlertRecipientSql } from '../../../packages/server-quality/src/adapters/quality-alert-recipient-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-quality/src');
const read = (path: string) => readFileSync(resolve(srcRoot, path), 'utf8');
const sha = (sql: string) => createHash('sha256').update(sql).digest('hex');

// Pre-dedupe assignment-recipient predicate copied byte-for-byte from HEAD.
const recipient = `m.company_id=h.company_id
 AND iam_assignment_active(m.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)
 AND iam_role_allows(m.role,m.company_id,CASE WHEN h.status IN ('awaiting_release','cancel_pending') THEN 'quality.release' ELSE 'quality.inspect' END)
 AND (m.scope_kind='holding' OR (m.scope_kind='branch' AND m.scope_node_id=h.branch_id))
 AND NOT iam_same_person(m.user_id,h.created_by,h.company_id)
 AND (h.status<>'cancel_pending' OR NOT iam_same_person(m.user_id,h.cancel_requested_by,h.company_id))
 AND (h.status<>'awaiting_release' OR NOT iam_same_person(m.user_id,h.last_inspector,h.company_id))`;

const assignments = identityAccessProjectionSources.assignments;

// Byte oracles captured at HEAD before the r34 fold, rendered by the retired
// inline SELECT. The r34 helper passes the identical FROM/WHERE and tail to
// evidenceHead, so both call sites stay byte-identical and keep their binds.
const renderings = [
  ['1', '\n             AND m.user_id=?',
    `SELECT 1 FROM (${assignments}) m WHERE ${recipient}\n             AND m.user_id=?`,
    'd22588168ff8289db5de55a20c6c7b0e40c5d52d30408eca475d3b800b5579d9'],
  ['COUNT(DISTINCT m.user_id)', '',
    `SELECT COUNT(DISTINCT m.user_id) FROM (${assignments}) m WHERE ${recipient}`,
    '50c5460bab1971f2db5a4c122e76e418927d28aa39d70a2ba20c0d38348a804e'],
] as const;

const kpi = (pattern: RegExp) => {
  let total = 0;
  const walk = (dir: string) => {
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
      const path = resolve(dir, entry.name);
      if (entry.isDirectory()) walk(path);
      else if (entry.name.endsWith('.ts')) {
        total += (readFileSync(path, 'utf8').match(pattern) ?? []).length;
      }
    }
  };
  walk(srcRoot);
  return total;
};

describe('quality read-tail r34 alert recipient head dedupe', () => {
  test.each(renderings)('projection %s is byte-identical to the pre-dedupe rendering', (projection, extra, before, digest) => {
    const sql = qualityAlertRecipientSql(projection, extra);
    expect(sql).toBe(before);
    expect(sha(sql)).toBe(digest);
    expect(sql).toBe(evidenceHead(projection, ` FROM (${assignments}) m WHERE ${recipient}`, extra));
    expect(sql.match(/SELECT /g)).toHaveLength(2);
    expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
  });

  test('the helper delegates to evidenceHead and owns no inline SELECT', () => {
    const helper = read('adapters/quality-alert-recipient-sql.ts');
    expect(helper).toContain('import { evidenceHead } from "./inspection-evidence-source-sql";');
    expect(helper).toContain('evidenceHead(projection, ` FROM (${identityAccessProjectionSources.assignments}) m WHERE ${recipient}`, extra)');
    expect(helper.match(/SELECT /g) ?? []).toHaveLength(0);
    const head = read('adapters/inspection-evidence-source-sql.ts');
    expect(head).toContain('export const evidenceHead = (projection: string, from: string, tail: string) => `SELECT ${projection}${from}${tail}`;');
    expect(head.match(/SELECT /g)).toHaveLength(1);
  });

  test('alerts.ts callers and the [userId, companyId, branchId, branchId] bind order are unchanged', () => {
    const alerts = read('adapters/alerts.ts');
    expect(alerts).toContain("import { qualityAlertRecipientSql } from './quality-alert-recipient-sql';");
    expect(alerts.match(/qualityAlertRecipientSql\(/g)).toHaveLength(2);
    expect(alerts).toContain('CASE WHEN EXISTS(${qualityAlertRecipientSql(');
    expect(alerts).toContain("(${qualityAlertRecipientSql('COUNT(DISTINCT m.user_id)')}) recipients");
    expect(alerts).toMatch(/\.bind\(actor\.userId, \.\.\.scopeArgs\(actor\)\)/);
    expect(alerts.replace(/\s+/g, ' ')).toContain('actor.companyId, actor.branchId, actor.branchId,');
  });

  test('retirement:ts-statements is strictly lower than the BEFORE=28 baseline', () => {
    expect(kpi(/INSERT INTO|DELETE FROM|UPDATE [a-z_]+ SET|SELECT /g)).toBe(19);
    expect(kpi(/SELECT /g)).toBe(19);
  });
});
