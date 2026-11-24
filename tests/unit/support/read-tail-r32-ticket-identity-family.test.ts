import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import { ticketByIdSql, ticketGuardExistsSql } from '../../../packages/server-support/src/adapters/ticket-guard-sources';

const srcRoot = resolve(__dirname, '../../../packages/server-support/src');
const src = (f: string) => readFileSync(resolve(srcRoot, f), 'utf8');
const helper = src('adapters/ticket-guard-sources.ts');
const evidence = src('adapters/support-evidence.ts');
const eligibility = '<ELIGIBLE>';
const predicate = '<PREDICATE>';

/** Exact HEAD (pre-r32) renderings: the guard witness and the evidence
 * eligibility witness both carried a private copy of the same ticket-identity
 * SELECT. Only the projection and the trailing predicate differed. */
const beforeGuard = `EXISTS(SELECT 1 FROM support_tickets t WHERE t.id=? AND ${predicate})`;
const beforeEligibility = `SELECT 1 AS ok FROM support_tickets t WHERE t.id=? AND ${eligibility}`;

function selectCount() {
    let total = 0;
    const walk = (dir: string) => {
        for (const entry of readdirSync(dir, { withFileTypes: true })) {
            const path = resolve(dir, entry.name);
            if (entry.isDirectory()) walk(path);
            else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT/g) ?? []).length;
        }
    };
    walk(srcRoot);
    return total;
}

describe('Support read-tail r32 ticket-identity parameterized family', () => {
    test('one canonical parameterized ticket-identity SELECT', () => {
        expect(helper.match(/export const ticketByIdSql\s*=/g) ?? []).toHaveLength(1);
        expect(helper).toContain('export const ticketByIdSql = (projection: string, predicate: string) =>');
        expect(helper.match(/SELECT \$\{projection\} FROM support_tickets t WHERE t\.id=\? AND \$\{predicate\}/g) ?? []).toHaveLength(1);
        expect(helper).toContain('export const ticketGuardExistsSql = (predicate: string) =>');
    });

    test('guard and evidence eligibility render byte-identically to HEAD', () => {
        expect(ticketGuardExistsSql(predicate)).toBe(beforeGuard);
        expect(ticketByIdSql('1 AS ok', eligibility)).toBe(beforeEligibility);
        expect(ticketByIdSql('1', predicate)).toBe(beforeGuard.slice('EXISTS('.length, -1));
    });

    test('callers compose the family with no private SELECT copy', () => {
        expect(evidence).toContain("import { ticketByIdSql, ticketGuardExistsSql } from './ticket-guard-sources';");
        expect(evidence).toContain("ticketByIdSql('1 AS ok', eligible.replaceAll('t.owner_id', '?::text'))");
        expect(evidence).not.toContain('SELECT 1 AS ok FROM support_tickets');
        expect(evidence.match(/ticketByIdSql\(/g) ?? []).toHaveLength(1);
        expect(src('adapters/support-ticket-read.ts')).toContain('ticketGuardExistsSql(');
        expect(src('adapters/support-ticket-read.ts')).not.toContain('SELECT 1 FROM support_tickets t WHERE t.id=?');
    });

    test('locks and native read kinds stay untouched', () => {
        expect(helper).not.toMatch(/LOCK TABLE|pg_advisory_xact_lock/);
        expect(evidence).not.toMatch(/LOCK TABLE|pg_advisory_xact_lock/);
        expect(evidence).toContain('nativeSupportReadRuntime');
        expect(evidence).toContain('SUPPORT_RD_EVIDENCE_ITEMS');
        expect(src('adapters/ticket-portal-unit.ts')).toContain('pg_advisory_xact_lock');
        expect(src('adapters/ticket-portal-sources.ts')).toContain('LOCK TABLE');
        expect(src('adapters/native-support-read.ts')).toContain("SUPPORT_RD_SLA_ALERTS = 'support_rd_sla_alerts'");
        expect(src('adapters/native-support-read.ts')).toContain("SUPPORT_RD_TICKET_RECIPIENTS = 'support_rd_ticket_recipients'");
    });

    test('retirement:ts-statements is strictly below the r32 BEFORE=46 baseline', () => {
        expect(selectCount()).toBe(44);
        expect(selectCount()).toBeLessThan(45);
    });
});
