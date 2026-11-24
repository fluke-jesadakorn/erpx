import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import {
  benefitDependentParticipants,
  benefitElectionParticipants,
} from '@erpx/server-people_payroll/public/benefits-owner-facts';
import { benefitPackageSettlementSources, materializedScopeCte } from '../../../packages/server-benefits/src/adapters/package-obligations';
import { sensitiveFactSelect } from '../../../packages/server-benefits/src/adapters/attachments-sensitive-facts';

const srcRoot = resolve(__dirname, '../../../packages/server-benefits/src');
const adapterDir = resolve(srcRoot, 'adapters');
const src = (name: string) => readFileSync(resolve(adapterDir, name), 'utf8');

// Pre-dedupe literal, copied from HEAD, used only as the byte-identity oracle.
function terminal(table: string, id: string, kind: string) {
 return `EXISTS(SELECT 1 FROM ${table} event JOIN command_receipts command ON command.record_id=${id}.id
 AND command.operation='benefits.${kind}.'||CASE WHEN event.action IN ('create','elect') THEN 'create' ELSE event.action END
 AND command.actor_id=event.actor_id AND command.authority_json::jsonb->>'companyId'=${id}.company_id
 AND command.result_json::jsonb->>'id'=${id}.id AND command.result_json::jsonb->>'status'=event.status
 AND command.result_json::jsonb->>'version'=event.version::text
 WHERE event.${kind === 'window' ? 'window' : kind}_id=${id}.id AND event.status=${id}.status AND event.version<=${id}.version)`;
}

const readOnly = (sql: string) => {
  expect(sql).toContain('SELECT ');
  expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
};

describe('Benefits read-tail r12 MATERIALIZED scope CTE family', () => {
  test.each([
    ['plans', 'p', 'plan', 'benefit_plan_events', 'benefit_plans'],
    ['windows', 'w', 'window', 'benefit_window_events', 'benefit_enrollment_windows'],
    ['elections', 'e', 'election', 'benefit_election_events', 'benefit_elections'],
  ] as const)('%s generated CTE stays byte-identical to the pre-dedupe literal', (key, alias, kind, events, table) => {
    const settlement = `SELECT ${alias}.*,${terminal(events, alias, kind)} terminal_proven FROM ${table} ${alias}`;
    expect(benefitPackageSettlementSources[key]).toBe(settlement);
    const expected = `SELECT ${alias}.* FROM (${settlement}) ${alias},scope WHERE ${alias}.company_id=scope.company_id`;
    expect(materializedScopeCte(alias, benefitPackageSettlementSources[key])).toBe(expected);
    readOnly(expected);
  });

  test('the CTE generator is one owner literal with three callers and no inlining', () => {
    const text = src('package-obligations.ts');
    expect(text.match(/export const materializedScopeCte=/g) ?? []).toHaveLength(1);
    expect(text.match(/materializedScopeCte\('/g) ?? []).toHaveLength(3);
    expect(text).not.toMatch(/MATERIALIZED\(SELECT/);
    const family = text.slice(text.indexOf('export const materializedScopeCte='), text.indexOf('export async function benefitPackageObligations'));
    expect(family).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
  });
});

describe('Benefits read-tail r12 sensitive-fact pair', () => {
  test('dependent and election reads stay byte-identical to the pre-dedupe literals', () => {
    const dependents = `SELECT d.company_id,d.version,d.status,e.branch_id,e.department_id,
        CASE WHEN iam_same_person(p.user_id,?,d.company_id) THEN 1 ELSE 0 END is_owner
      FROM benefit_dependents d
      ${benefitDependentParticipants()}
      WHERE d.id=? AND d.company_id=?`;
    const elections = `SELECT x.company_id,x.version,x.status,e.branch_id,e.department_id,
        CASE WHEN iam_same_person(p.user_id,?,x.company_id) THEN 1 ELSE 0 END is_owner
      FROM benefit_elections x
      ${benefitElectionParticipants()}
      WHERE x.id=? AND x.company_id=?`;
    expect(sensitiveFactSelect('d', 'benefit_dependents', benefitDependentParticipants)).toBe(dependents);
    expect(sensitiveFactSelect('x', 'benefit_elections', benefitElectionParticipants)).toBe(elections);
    readOnly(dependents);
    readOnly(elections);
  });

  test('the sensitive-fact generator is one owner literal with two callers and no inlining', () => {
    const text = src('attachments-sensitive-facts.ts');
    expect(text.match(/export const sensitiveFactSelect=/g) ?? []).toHaveLength(1);
    expect(text.match(/sensitiveFactSelect\('/g) ?? []).toHaveLength(2);
    expect(text).not.toMatch(/SELECT (d|x)\.company_id,(d|x)\.version/);
    const family = text.slice(text.indexOf('export const sensitiveFactSelect='));
    expect(family).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
  });
});

describe('Benefits read-tail r12 retirement check', () => {
  test('retirement:ts-statements stays strictly below the r12 BEFORE=60 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT/g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(60);
  });
});
