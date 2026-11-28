import { describe, expect, test, vi } from 'vitest';
import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
vi.mock('server-only', () => ({}));
import { packageSourceProjection } from '../../../modules/commercial/server/src/adapters/commercial-read-tail-r11-sources';
import { purchaseScopeRead } from '../../../modules/commercial/server/src/adapters/commercial-read-tail-r12-sources';
import { purchaseObservationAgg } from '../../../modules/commercial/server/src/adapters/commercial-read-tail-r6-sources';
import { packageSourceSelect } from '../../../modules/commercial/server/src/adapters/commercial-read-tail-r27-sources';

const srcRoot = resolve(__dirname, '../../../modules/commercial/server/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
const sha = (sql: string) => createHash('sha256').update(sql).digest('hex');
// Prose comments describe the excluded lock kinds; assertions inspect code only.
const codeOnly = (text: string) => text.replace(/\/\*[\s\S]*?\*\//g, '').replace(/\/\/[^\n]*/g, '');
const readOnly = (sql: string) => {
  expect(sql).toContain('SELECT ');
  expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
};

// Byte oracles captured from HEAD before the r27 fold. The byte-identical head
// `SELECT <projection> FROM <source><tail>` was duplicated in the r11
// package-source projection and the r12 purchase-scope read; both now delegate
// to the single owner-local packageSourceSelect renderer. Rendered SQL, bind
// order and every caller-side lock statement are unchanged.
const company = 'scope.company_id';
const now = 'scope.now';
const predicate = 'purchase_scope.company_id IS NOT NULL';
const grants = purchaseObservationAgg('purchase_scope.company_id', 'company_package_grants', 'g', "'companyId',g.company_id,'packageId',g.package_id,'packageVersion',g.package_version,'state',g.state,'startsAt',g.starts_at,'endsAt',g.ends_at,'version',g.version");
const history = purchaseObservationAgg('purchase_scope.company_id', 'company_package_history_rights', 'h', "'companyId',h.company_id,'packageId',h.package_id,'packageVersion',h.package_version,'sourceGrantVersion',h.source_grant_version,'sourceEventId',h.source_event_id,'retainedAt',h.retained_at");
const GRANTS = `SELECT package_id,package_version FROM company_package_grants WHERE company_id=${company} AND starts_at<=${now} AND state<>'inactive'`;
const HISTORY = `SELECT package_id,package_version FROM company_package_history_rights WHERE company_id=${company} AND retained_at<=${now}
        AND retained_at>=0 AND retained_at<=9007199254740991 AND source_grant_version>0 AND source_event_id<>''`;
const SOURCES = `${GRANTS}\n      UNION ${HISTORY}`;
const GUARD_PROJ = `CASE WHEN ${predicate} THEN 1 ELSE 0 END ok`;
const OBS_PROJ = `true commercial_purchase_observation,purchase_scope.company_id,?::text capability,?::text purpose,${predicate} allowed,purchase_scope.now observed_at,
    ${grants} grants,
    ${history} history`;
const GUARD_SQL = `SELECT ${GUARD_PROJ} FROM purchase_scope`;
const OBS_SQL = `SELECT ${OBS_PROJ} FROM purchase_scope`;
const ORACLES: Array<[string, string]> = [
  [GRANTS, 'c8bc6bbdc6358922200c196b39c748b088fcdec1024cbf15c0100bffb41f8a4e'],
  [HISTORY, '9216e8e33b3f6fbb28fb1bbb96c8d5b732d97b0467eb54d5e82f526e8d007a85'],
  [SOURCES, '5f1edb437021f0c2d164fc7b99a944ed7ca030861ede96d55dda7f558b737943'],
  [GUARD_SQL, '98af1d2d780f284ab8e5936f37abdb0d3182f85e20cb5adec359bd73fd16ba92'],
  [OBS_SQL, '0e194a8c488d93a8b106f96e88f27c947ba1a70d84ba068ef8b711bb1b1b1932'],
];

describe('Commercial read-tail r27 package-source head dedupe', () => {
  test('both delegated renderers stay byte-identical to the pre-dedupe oracles', () => {
    const produced = [
      packageSourceProjection('company_package_grants', company, `starts_at<=${now} AND state<>'inactive'`),
      packageSourceProjection('company_package_history_rights', company, `retained_at<=${now}
        AND retained_at>=0 AND retained_at<=9007199254740991 AND source_grant_version>0 AND source_event_id<>''`),
      `${packageSourceProjection('company_package_grants', company, `starts_at<=${now} AND state<>'inactive'`)}\n      UNION ${packageSourceProjection('company_package_history_rights', company, `retained_at<=${now}\n        AND retained_at>=0 AND retained_at<=9007199254740991 AND source_grant_version>0 AND source_event_id<>''`)}`,
      purchaseScopeRead(GUARD_PROJ),
      purchaseScopeRead(OBS_PROJ),
    ];
    for (const [i, actual] of produced.entries()) {
      expect(actual, ORACLES[i][0]).toBe(ORACLES[i][0]);
      expect(Buffer.from(actual)).toEqual(Buffer.from(ORACLES[i][0]));
      expect(sha(actual), ORACLES[i][1]).toBe(ORACLES[i][1]);
      readOnly(actual);
    }
    expect(produced[2]).toBe(SOURCES);
    expect((produced[4].match(/\?/g) ?? []).length).toBe(2); // capability + purpose binds
  });

  test('one byte-identical head is owned by the owner-local r27 renderer', () => {
    const head = src('commercial-read-tail-r27-sources.ts');
    expect(head.match(/SELECT /g)).toHaveLength(1); // single local head literal
    expect(codeOnly(head)).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    expect(head).not.toContain('@erpx/server-'); // no cross-owner import
    expect(packageSourceSelect('package_id,package_version', 'company_package_grants', ' WHERE company_id=?')).toBe('SELECT package_id,package_version FROM company_package_grants WHERE company_id=?');
    expect(packageSourceSelect(GUARD_PROJ, 'purchase_scope', '')).toBe(GUARD_SQL);

    const r11 = src('commercial-read-tail-r11-sources.ts');
    expect(r11).toContain('import { packageSourceSelect } from "./commercial-read-tail-r27-sources"');
    expect(r11).toContain('packageSourceSelect("package_id,package_version", source, ` WHERE company_id=${company} AND ${predicate}`)');
    expect(r11.match(/SELECT /g) ?? []).toHaveLength(0); // no local head remains
    const r12 = src('commercial-read-tail-r12-sources.ts');
    expect(r12).toContain('import { packageSourceSelect } from "./commercial-read-tail-r27-sources"');
    expect(r12).toContain('packageSourceSelect(projection, "purchase_scope", "")');
    expect(r12.match(/SELECT /g) ?? []).toHaveLength(0);
  });

  test('no write/lock/native/cross-owner surface was introduced', () => {
    const pg = src('package-grants.ts');
    expect(pg).toContain('SELECT pg_advisory_xact_lock(hashtextextended(?,0))');
    expect(pg).toContain('SELECT version FROM company_package_revisions WHERE company_id=? FOR UPDATE');
    const pcb = src('purchase-command-boundary.ts');
    expect(pcb).toContain('SELECT pg_advisory_xact_lock(hashtextextended(?,0))');
    expect(pcb).toContain("SELECT pg_advisory_xact_lock(hashtextextended('commercial.package:company:'||company_id,0))");
    expect(pcb).toContain('SELECT package_id FROM company_package_grants WHERE company_id=? ORDER BY package_id FOR SHARE');
    expect(pcb).toContain('SELECT company_id,package_id FROM company_package_grants ORDER BY company_id,package_id FOR SHARE');
    expect(pcb).toContain('FOR SHARE');
    expect(pcb).toContain("nativeOwnerStatement('purchase.assertion'");
    expect(src('../public/native_read.rs')).toContain('caller: "commercial-read-tail-r5-sources.receiptProbe"');
  });

  test('caller-side prepare/bind and lock tails are unchanged', () => {
    const consumer = src('purchased-capability-sql.ts');
    expect(consumer.match(/packageSourceProjection\(/g)).toHaveLength(2);
    expect(consumer).toContain('packageSourceProjection("company_package_grants"');
    expect(consumer).toContain('UNION ${packageSourceProjection(');
    expect(consumer.match(/packageSourceProbe\(/g)).toHaveLength(4);
    expect(consumer).not.toContain('SELECT package_id,package_version FROM');

    const pcb = src('purchase-command-boundary.ts');
    expect(pcb.match(/purchaseScopeRead\(/g)).toHaveLength(2);
    expect(pcb.match(/SELECT /g)).toHaveLength(6); // clock CTE + pinned lock statements only
    expect(pcb).toContain('clock + purchaseScopeRead(`CASE WHEN');
    expect(pcb).toContain('clock + purchaseScopeRead(`true commercial_purchase_observation');
    expect(pcb).not.toContain('SELECT CASE WHEN');
    expect(pcb).not.toContain('SELECT true commercial_purchase_observation');
    expect(pcb).toContain('.bind(input.companyId, input.capability, input.purpose)');
  });

  test('retirement:ts-statements pins 12 -> 11', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBe(11);
    expect(total).toBeLessThan(14);
  });
});
