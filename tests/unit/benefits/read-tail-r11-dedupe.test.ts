import { describe, expect, test } from 'vitest';
import { readFileSync } from 'node:fs';
import { benefitPackageSettlementSources } from '../../../modules/benefits/server/src/adapters/package-obligations';

const src = readFileSync(new URL('../../../modules/benefits/server/src/adapters/package-obligations.ts', import.meta.url), 'utf8');

// Pre-dedupe literal, copied from HEAD, used only as the byte-identity oracle.
function terminal(table: string, id: string, kind: string) {
 return `EXISTS(SELECT 1 FROM ${table} event JOIN command_receipts command ON command.record_id=${id}.id
 AND command.operation='benefits.${kind}.'||CASE WHEN event.action IN ('create','elect') THEN 'create' ELSE event.action END
 AND command.actor_id=event.actor_id AND command.authority_json::jsonb->>'companyId'=${id}.company_id
 AND command.result_json::jsonb->>'id'=${id}.id AND command.result_json::jsonb->>'status'=event.status
 AND command.result_json::jsonb->>'version'=event.version::text
 WHERE event.${kind === 'window' ? 'window' : kind}_id=${id}.id AND event.status=${id}.status AND event.version<=${id}.version)`;
}

describe('Benefits read-tail r11 parameterized settlement-source family', () => {
  test.each([
    ['plans', 'benefit_plan_events', 'p', 'plan', 'benefit_plans'],
    ['windows', 'benefit_window_events', 'w', 'window', 'benefit_enrollment_windows'],
    ['dependents', 'benefit_dependent_events', 'd', 'dependent', 'benefit_dependents'],
    ['elections', 'benefit_election_events', 'e', 'election', 'benefit_elections'],
  ] as const)('%s generated SQL stays byte-identical to the pre-dedupe literal', (key, events, alias, kind, table) => {
    const expected = `SELECT ${alias}.*,${terminal(events, alias, kind)} terminal_proven FROM ${table} ${alias}`;
    expect(benefitPackageSettlementSources[key]).toBe(expected);
  });

  test('inputs are unchanged and the family has one owner literal with four callers', () => {
    expect(benefitPackageSettlementSources.inputs).toBe('SELECT * FROM benefit_payroll_inputs');
    expect(src.match(/const settlementSource=/g) ?? []).toHaveLength(1);
    expect(src.match(/settlementSource\('benefit_/g) ?? []).toHaveLength(4);
    for (const kind of ['plan', 'window', 'dependent', 'election']) {
      expect(src).not.toContain(`SELECT ${kind === 'window' ? 'w' : kind === 'dependent' ? 'd' : kind === 'election' ? 'e' : 'p'}.*,${'${terminal('}`);
    }
  });

  test('the family adds no write or lock statement', () => {
    const family = src.slice(src.indexOf('const settlementSource='), src.indexOf('/** Closed actual owner lineage'));
    expect(family).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
  });
});
