import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import {
  socialPurchaseActorFeature,
  socialPurchaseActorExists,
} from '../../../modules/collaboration/server/src/adapters/read-tail-sql';

const srcRoot = resolve(__dirname, '../../../modules/collaboration/server/src');
const adapters = resolve(srcRoot, 'adapters');
const shared = readFileSync(resolve(adapters, 'read-tail-sql.ts'), 'utf8');
const readPolicy = readFileSync(resolve(adapters, 'work-social-read-policy.ts'), 'utf8');

describe('Collaboration read-tail r17 parameterized family', () => {
  test('socialPurchaseActorExists stays byte-identical to the four pre-dedupe EXISTS literals', () => {
    const collab = "scheduledFeaturePredicate('social_purchase_actor.company_id','collaboration')";
    expect(socialPurchaseActorExists(`${collab} AND TRUE`)).toBe(
      `EXISTS(SELECT 1 FROM social_purchase_actor WHERE ${collab} AND TRUE)`,
    );
    expect(socialPurchaseActorExists('projectRead(purpose,true)')).toBe(
      'EXISTS(SELECT 1 FROM social_purchase_actor WHERE projectRead(purpose,true))',
    );
    const nested = "NOT EXISTS(SELECT 1 FROM (sourceRows) source WHERE source.kind='pms_record')";
    expect(socialPurchaseActorExists(nested)).toBe(
      `EXISTS(SELECT 1 FROM social_purchase_actor WHERE ${nested})`,
    );
    const disjunction = "NOT (operationalHandoverReceiptPredicate('p')) OR TRUE";
    expect(socialPurchaseActorExists(disjunction)).toBe(
      `EXISTS(SELECT 1 FROM social_purchase_actor WHERE ${disjunction})`,
    );
  });

  test('socialPurchaseActorExists reuses the bare feature fragment', () => {
    expect(socialPurchaseActorExists('P')).toBe(`EXISTS(${socialPurchaseActorFeature('P')})`);
  });

  test('family is defined once in the shared file', () => {
    expect(shared.match(/export const socialPurchaseActorExists=/g) ?? []).toHaveLength(1);
  });

  test('inline EXISTS literals are gone and the read policy calls the helper four times', () => {
    expect(readPolicy.split('socialPurchaseActorExists(').length - 1).toBe(4);
    expect(readPolicy).not.toContain('EXISTS(SELECT 1 FROM social_purchase_actor WHERE ');
  });
});

describe('Collaboration read-tail r17 retirement check', () => {
  test('retirement:ts-statements stays strictly below the r17 BEFORE=419 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT/g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(419);
  });
});
