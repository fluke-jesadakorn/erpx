import { describe, expect, test, vi } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
vi.mock('server-only', () => ({}));
import { commercialOwnerProjectionSources as identityAccessProjectionSources } from '@erpx/server-identity_access/public/commercial-owner-projection-sources';
import { qualityAlertRecipientSql } from '../../../packages/server-quality/src/adapters/quality-alert-recipient-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-quality/src');
const read = (path: string) => readFileSync(resolve(srcRoot, path), 'utf8');

// Pre-dedupe assignment-recipient predicate copied byte-for-byte from HEAD.
const recipient = `m.company_id=h.company_id
 AND iam_assignment_active(m.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)
 AND iam_role_allows(m.role,m.company_id,CASE WHEN h.status IN ('awaiting_release','cancel_pending') THEN 'quality.release' ELSE 'quality.inspect' END)
 AND (m.scope_kind='holding' OR (m.scope_kind='branch' AND m.scope_node_id=h.branch_id))
 AND NOT iam_same_person(m.user_id,h.created_by,h.company_id)
 AND (h.status<>'cancel_pending' OR NOT iam_same_person(m.user_id,h.cancel_requested_by,h.company_id))
 AND (h.status<>'awaiting_release' OR NOT iam_same_person(m.user_id,h.last_inspector,h.company_id))`;

describe('quality alert recipient family dedupe (r17)', () => {
  test('one parameterized helper reproduces both recipient SELECTs byte-for-byte', () => {
    const assignments = identityAccessProjectionSources.assignments;
    expect(qualityAlertRecipientSql('1', '\n             AND m.user_id=?'))
      .toBe(`SELECT 1 FROM (${assignments}) m WHERE ${recipient}\n             AND m.user_id=?`);
    expect(qualityAlertRecipientSql('COUNT(DISTINCT m.user_id)'))
      .toBe(`SELECT COUNT(DISTINCT m.user_id) FROM (${assignments}) m WHERE ${recipient}`);
  });
  test('the alert readers share the helper with no inline recipient SELECTs', () => {
    const text = read('adapters/alerts.ts');
    expect(text.match(/qualityAlertRecipientSql\(/g)).toHaveLength(2);
    expect(text).not.toContain('SELECT 1 FROM (');
    expect(text).not.toContain('SELECT COUNT(DISTINCT m.user_id)');
    expect(text).toContain('CASE WHEN EXISTS(${qualityAlertRecipientSql(');
    expect(text).toContain("(${qualityAlertRecipientSql('COUNT(DISTINCT m.user_id)')}) recipients");
    expect(read('adapters/quality-alert-recipient-sql.ts').match(/SELECT /g) ?? []).toHaveLength(0);
  });
  test('the helper is read-only; descriptor cutover, locks and r1-r16 helpers stay untouched', () => {
    const helper = read('adapters/quality-alert-recipient-sql.ts');
    expect(helper).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
    expect(read('adapters/purchase-sources.ts')).toContain('qualityPurchaseSourceReadLocks');
    expect(read('adapters/attachment-read-sources.ts')).toContain('LOCK TABLE inventory_quality_holds,inventory_quality_specs');
    for (const name of ['quality-work-scope-sql', 'native-source-wrapper-sql', 'inspection-evidence-source-sql', 'quality-lineage-recursion-sql']) {
      expect(read(`adapters/${name}.ts`).match(/SELECT /g) ?? []).toHaveLength(name === 'native-source-wrapper-sql' ? 0 : 1);
    }
  });
});

describe('quality read-tail r17 retirement check', () => {
  test('retirement:ts-statements is strictly lower than the BEFORE=75 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/INSERT INTO|DELETE FROM|UPDATE [a-z_]+ SET|SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(75);
    expect(total).toBe(21);
  });
});
