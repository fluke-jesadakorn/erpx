import { describe, expect, test } from 'vitest';
import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { planningAttachmentTargetRow } from '../../../modules/planning/server/src/adapters/planning-attachment-target-sql';

const srcRoot = resolve(__dirname, '../../../modules/planning/server/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
const selectCount = (text: string) => (text.match(/SELECT /g) ?? []).length;
const sha = (text: string) => createHash('sha256').update(text).digest('hex');

// Verbatim pre-r29 renderings of the attachment-target row head. They are
// byte-identity oracles only, never live query sources.
const OPERATIONAL = 'SELECT company_id,branch_id branch_id,NULL::text department_id,version FROM planning_scenarios WHERE id=? AND company_id=?';
const POLICY = 'SELECT company_id,branch_id AS branch_id,NULL AS department_id,version AS version FROM planning_scenarios WHERE id=? AND company_id=?';
const SCOPED = 'SELECT company_id,version FROM planning_scenarios WHERE id=? AND company_id=? AND branch_id=?';

describe('Planning r29 attachment-target SELECT head retirement', () => {
  test('the generalized fact builder reproduces every retired row statement byte-for-byte', () => {
    const operational = planningAttachmentTargetRow('branch_id branch_id,NULL::text department_id,version', 'planning_scenarios');
    const policy = planningAttachmentTargetRow('branch_id AS branch_id,NULL AS department_id,version AS version', 'planning_scenarios', '');
    const scoped = planningAttachmentTargetRow('version', 'planning_scenarios', ' AND branch_id=?');
    expect(operational).toBe(OPERATIONAL);
    expect(policy).toBe(POLICY);
    expect(scoped).toBe(SCOPED);
    expect([operational, policy, scoped].map(sha)).toEqual([
      '581d7b62dbf1c791cba4b70b7aa9b89deacae9803f285fbd8f32ee6e265b5e33',
      'd051b20520618885d2f9b70e1bf6655a0f05e2fa6fbeb69e22e21d8a218c31dd',
      'f7787d5c9eeb8221bf4bef4834972e80edaee2e82d2d9f4145c929220a73cae8',
    ]);
    for (const sql of [operational, policy, scoped]) {
      expect(selectCount(sql)).toBe(1);
      expect(sql.toUpperCase()).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    }
  });

  test('the retired head is gone; the thin wrapper keeps its two exports and no SQL literal', () => {
    const wrapper = src('planning-attachment-target-sql.ts');
    expect(selectCount(wrapper)).toBe(0);
    expect(wrapper).toContain('planningPackageFactRows');
    expect(wrapper).toContain('export const planningAttachmentTargetRow =');
    expect(wrapper).toContain('export const planningAttachmentTargetVersionExists =');
    for (const name of ['attachments-policy-facts.ts', 'attachments-service-facts.ts', 'attachments-operational-facts.ts'])
      expect(src(name)).toContain('planningAttachmentTarget');
  });

  test('retirement:ts-statements drops 11 -> 10', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += selectCount(readFileSync(path, 'utf8'));
      }
    };
    walk(srcRoot);
    expect(total).toBe(5);
    expect(total).toBeLessThan(11);
  });
});
