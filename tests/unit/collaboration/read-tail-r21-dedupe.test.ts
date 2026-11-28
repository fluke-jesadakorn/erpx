import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import { projectPurchaseSourceAbsent } from '../../../modules/collaboration/server/src/adapters/read-tail-sql';

const srcRoot = resolve(__dirname, '../../../modules/collaboration/server/src');
const adapters = resolve(srcRoot, 'adapters');
const shared = readFileSync(resolve(adapters, 'read-tail-sql.ts'), 'utf8');
const readPolicy = readFileSync(resolve(adapters, 'work-social-read-policy.ts'), 'utf8');
// Byte-identical project-purchase source-absence guard that appeared in both
// read-policy call sites in work-social-read-policy.ts. Only the trailing
// authorization predicate differs, so it is parameterized and the binds and
// lock semantics stay in the same order.
const SRC = '(source-rows)';
const PURCH = '(purchase-sources)';
const PREDICATE = '(authorization-predicate)';
const EXPECTED = `SELECT 1 FROM (${SRC}) source JOIN (${PURCH}) social_project_source ON social_project_source.source_kind=source.kind AND social_project_source.source_id=source.id AND social_project_source.company_id=p.company_id WHERE NOT (${PREDICATE})`;
const INLINE = `SELECT 1 FROM (\${sourceRows}) source JOIN (\${projectSocialPurchaseSources}) social_project_source ON social_project_source.source_kind=source.kind AND social_project_source.source_id=source.id AND social_project_source.company_id=p.company_id WHERE NOT (`;

describe('Collaboration read-tail r21 parameterized project-purchase source-absence family', () => {
  test('projectPurchaseSourceAbsent stays byte-identical to both pre-dedupe literals', () => {
    expect(projectPurchaseSourceAbsent(SRC, PURCH, PREDICATE)).toBe(EXPECTED);
    // The guard contributes exactly one SELECT; the joined fragments are supplied by callers.
    expect(projectPurchaseSourceAbsent(SRC, PURCH, PREDICATE).split('SELECT ').length - 1).toBe(1);
    expect(projectPurchaseSourceAbsent(SRC, PURCH, PREDICATE).split('JOIN (').length - 1).toBe(1);
  });

  test('the shared fragment is declared exactly once and the inline literals are gone', () => {
    expect(shared.match(/export const projectPurchaseSourceAbsent=/g) ?? []).toHaveLength(1);
    // one import + two call sites
    expect(readPolicy.split('projectPurchaseSourceAbsent').length - 1).toBe(3);
    expect(readPolicy).not.toContain(INLINE);
  });

  test('keeps the fragment read-only and leaves FOR UPDATE / lock semantics untouched', () => {
    const lockRe = /INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i;
    expect(projectPurchaseSourceAbsent(SRC, PURCH, PREDICATE)).not.toMatch(lockRe);
    for (const line of readPolicy.split('\n').filter(l => l.includes('projectPurchaseSourceAbsent(') && !l.includes('import'))) {
      expect(line).not.toMatch(lockRe);
    }
  });
});

describe('Collaboration read-tail r21 retirement check', () => {
  test('retirement:ts-statements stays strictly below the r21 BEFORE=411 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    // r21 deduped the 2x byte-identical project-purchase source-absence guard
    // in work-social-read-policy.ts into one shared parameterized fragment.
    expect(total).toBeLessThan(411);
  });
});
