import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { activeRecipientsSql } from '../../../modules/identity-access/server/src/adapters/iam-probe-sql';

const srcRoot = resolve(__dirname, '../../../modules/identity-access/server/src');
const sqlSrc = readFileSync(resolve(srcRoot, 'adapters/iam-probe-sql.ts'), 'utf8');

// Pre-dedupe literals copied from HEAD, used only as byte-identity oracles.
const pre = (gap: string) => `SELECT DISTINCT user_id FROM assignments WHERE company_id=? AND user_id IN (?,?)${gap}AND revoked_at IS NULL AND iam_assignment_active(id,?) AND iam_principal_active(user_id,company_id,?)`;

describe('Identity access read-tail r13 active-recipient family', () => {
  test.each([
    ['calendar availability', '\n    '],
    ['calendar commands', '\n        '],
    ['collaboration inbox', '\n      '],
  ] as const)('%s renders byte-identical SQL at its original gap', (_name, gap) => {
    expect(activeRecipientsSql(['u1', 'u2'], gap)).toBe(pre(gap));
  });

  test('the family is one owner literal with three callers and no write or lock', () => {
    expect(srcRoot).toBeTruthy();
    const family = sqlSrc.slice(sqlSrc.indexOf('export const activeRecipientsSql'));
    expect(family).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
    const usages = [
      'adapters/calendar-availability-facts.ts',
      'adapters/calendar-commands-facts.ts',
      'adapters/collaboration-inbox-sources.ts',
    ].map(f => readFileSync(resolve(srcRoot, f), 'utf8'));
    expect(usages.reduce((n, s) => n + (s.match(/activeRecipientsSql\(/g) ?? []).length, 0)).toBe(3);
    expect((sqlSrc.match(/export const activeRecipientsSql =/g) ?? []).length).toBe(1);
  });
});

describe('Identity access read-tail r13 retirement check', () => {
  test('retirement:ts-statements stays strictly below the r13 BEFORE=542 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) {
          total += (readFileSync(path, 'utf8').match(/INSERT INTO|DELETE FROM|UPDATE [a-z_]+ SET|SELECT /g) ?? []).length;
        }
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(542);
  });
});
