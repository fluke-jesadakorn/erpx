import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { hospitalityOwnerProjectionSources } from '@erpx/commerce-server/public/hospitality-owner-projection-sources';
import { COMMERCE_DELIVERY_GUARD_SQL } from '../../../modules/files/server/src/adapters/commerce-delivery-dedupe-sql';
import { ATTACHMENT_ID_AND_HEAD_SQL } from '../../../modules/files/server/src/adapters/read-tail-r43-source-sql';

const srcRoot = resolve(__dirname, '../../../modules/files/server/src');
const adapterDir = resolve(srcRoot, 'adapters');
const src = (name: string) => readFileSync(resolve(adapterDir, name), 'utf8');

const PROJ = hospitalityOwnerProjectionSources.commerce_fulfillment_evidence;
const GUARD = `a.owner_type='commerce_order_delivery' AND a.owner_id=? AND a.purpose='proof_of_delivery' AND a.revoked_at IS NULL AND NOT EXISTS(SELECT 1 FROM attachment_links canonical WHERE canonical.attachment_id=a.id) AND NOT EXISTS(SELECT 1 FROM (${PROJ}) e WHERE e.attachment_id=a.id)`;
const REF = '${COMMERCE_DELIVERY_GUARD_SQL}';

const readOnly = (sql: string) => {
  expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory|\bcount\s*\(/i);
};

const selectCount = (text: string) => (text.match(/SELECT\s/g) ?? []).length;
const literalSelectCount = (text: string) => (text.match(/SELECT /g) ?? []).length;

describe('Files read-tail r23 parameterized-family dedupe', () => {
  test('canonical helper is the exact byte-identical commerce delivery guard', () => {
    expect(COMMERCE_DELIVERY_GUARD_SQL).toBe(GUARD);
    readOnly(COMMERCE_DELIVERY_GUARD_SQL);
    // Rendering reproduces the three original inlined literal bytes exactly.
    expect(`FROM attachments a WHERE ${COMMERCE_DELIVERY_GUARD_SQL} ORDER BY a.created_at DESC,a.id`)
      .toBe(`FROM attachments a WHERE ${GUARD} ORDER BY a.created_at DESC,a.id`);
    expect(`FROM attachments a WHERE a.id=? AND ${COMMERCE_DELIVERY_GUARD_SQL}`)
      .toBe(`FROM attachments a WHERE a.id=? AND ${GUARD}`);
    expect(`EXISTS(SELECT 1 FROM attachments a WHERE a.id=? AND ${COMMERCE_DELIVERY_GUARD_SQL})`)
      .toBe(`EXISTS(SELECT 1 FROM attachments a WHERE a.id=? AND ${GUARD})`);
  });

  test('helper is defined exactly once in the canonical owner', () => {
    expect(src('commerce-delivery-dedupe-sql.ts').split('COMMERCE_DELIVERY_GUARD_SQL =')).toHaveLength(2);
  });

  test('guard is composed, never inlined, in all three call sites', () => {
    const cf = src('commerce-facts.ts');
    const rendered = cf.replaceAll('${ATTACHMENT_ID_AND_HEAD_SQL}', ATTACHMENT_ID_AND_HEAD_SQL);
    expect(cf).not.toContain(GUARD.slice(0, 60));
    expect(cf.match(/\$\{COMMERCE_DELIVERY_GUARD_SQL\}/g) ?? []).toHaveLength(3);
    expect(rendered).toContain(`FROM attachments a WHERE ${REF} ORDER BY a.created_at DESC,a.id`);
    expect(rendered).toContain(`FROM attachments a WHERE a.id=? AND ${REF}`);
    expect(rendered).toContain(`EXISTS(SELECT 1 FROM attachments a WHERE a.id=? AND ${REF})`);
  });

  test('advisory locks, count(*) pairs and r10-r22 helpers remain untouched', () => {
    const sr = src('attachments/service-repository.ts');
    expect(sr).toContain('const advisoryLockSql = "SELECT pg_advisory_xact_lock(hashtextextended(?,0))"');
    expect(sr).toContain('const setLockSql = "SELECT state FROM attachment_sets WHERE id=? AND company_id=? FOR UPDATE"');
    expect(sr).toContain('SELECT count(DISTINCT candidate.id)');
    const tail = src('read-tail-dedupe-sql.ts');
    for (const name of [
      'attachmentSetPurposeSql', 'workforceActorScopeSql', 'scansAggSql', 'SCANS_WITNESS_SQL',
      'NO_ATTACHMENT_LINK_SQL', 'LINK_SET_ATTACHMENT_EXISTS_SQL', 'LINK_SET_ID_SELECT_SQL',
      'LINK_ATTACHMENT_SELECT_SQL', 'LINK_SET_WHERE_EXISTS_SQL', 'PACKAGE_OBLIGATION_SCOPE_SQL',
    ]) expect(tail).toContain(`export const ${name} =`);
  });

  test('retirement:ts-statements stays strictly below the r23 BEFORE=304 baseline', () => {
    let total = 0;
    let literal = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) {
          const text = readFileSync(path, 'utf8');
          total += selectCount(text);
          literal += literalSelectCount(text);
        }
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(304);
    expect(literal).toBeLessThan(304);
  });
});
