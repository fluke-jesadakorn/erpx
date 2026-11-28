import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import { ticketGuardExistsSql } from '../../../modules/support/server/src/adapters/ticket-guard-sources';

const srcRoot = resolve(__dirname, '../../../modules/support/server/src');
const src = (p: string) => readFileSync(resolve(srcRoot, p), 'utf8');
const ticketRead = src('adapters/support-ticket-read.ts');
const evidence = src('adapters/support-evidence.ts');
const helper = src('adapters/ticket-guard-sources.ts');

const access = '<ACCESS>';
const eligible = '<ELIGIBLE>';

/** Exact pre-dedupe (r22 BEFORE=57) guard assertions from HEAD. Each trailing
 * predicate is passed through untouched; only the shared ticket identity
 * prefix is factored into one helper. */
const beforeTicket = `EXISTS(SELECT 1 FROM support_tickets t WHERE t.id=? AND t.version=? AND t.company_id=? AND ${access})`;
const beforeEvidence = `EXISTS(SELECT 1 FROM support_tickets t WHERE t.id=? AND t.company_id=? AND t.version=? AND t.status IN ('open','in_progress','waiting_customer') AND (?::text IS NULL OR t.branch_id=?) AND (?::text IS NULL OR t.department_id=?) AND ${eligible})`;

describe('Support read-tail r22 ticket guard family', () => {
    test('authors one parameterized ticket guard witness', () => {
        expect(helper.match(/const ticketGuardExistsSql\s*=/g) ?? []).toHaveLength(1);
        expect(helper).toContain('export const ticketGuardExistsSql = (predicate: string) =>');
        expect(helper).toContain('export const ticketByIdSql = (projection: string, predicate: string) =>');
        expect(helper).toContain('`SELECT ${projection} FROM support_tickets t WHERE t.id=? AND ${predicate}`');
        expect(helper).toContain("`EXISTS(${ticketByIdSql('1', predicate)})`");
    });

    test('both guards render byte-identically with unchanged binds', () => {
        expect(ticketGuardExistsSql(`t.version=? AND t.company_id=? AND ${access}`)).toBe(beforeTicket);
        expect(ticketGuardExistsSql(`t.company_id=? AND t.version=? AND t.status IN ('open','in_progress','waiting_customer') AND (?::text IS NULL OR t.branch_id=?) AND (?::text IS NULL OR t.department_id=?) AND ${eligible}`)).toBe(beforeEvidence);
        expect(ticketRead).toContain('ticketGuardExistsSql(`t.version=? AND t.company_id=? AND ${access.sql}`)');
        expect(evidence).toContain(`ticketGuardExistsSql(\`t.company_id=? AND t.version=? AND t.status IN ('open','in_progress','waiting_customer')`);
        expect(ticketRead.match(/ticketGuardExistsSql\(/g) ?? []).toHaveLength(1);
        expect(evidence.match(/ticketGuardExistsSql\(/g) ?? []).toHaveLength(1);
        expect(ticketRead).toContain('[id, version, a.companyId, ...access.bindings]');
        expect(evidence).toContain('[id, a.companyId, expectedVersion, a.branchId, a.branchId, a.departmentId, a.departmentId, a.userId]');
        expect(ticketRead).not.toContain('EXISTS(SELECT 1 FROM support_tickets t WHERE t.id=? AND t.version');
        expect(evidence).not.toContain("EXISTS(SELECT 1 FROM support_tickets t WHERE t.id=? AND t.company_id");
    });

    test('keeps the earlier helpers and the lock/purchase/native kinds intact', () => {
        expect(src('adapters/service-access.ts')).toContain('const serviceTeamSetSql = (table: string, fields: string, order: string) =>');
        expect(src('adapters/service-access.ts')).toContain('SUPPORT_RD_SERVICE_SCOPE_COMPANY');
        expect(src('adapters/eligibility-projection-sources.ts')).toContain('export const supportAssignmentAccessSql = (alias: string, pad: string, tail: string) =>');
        expect(src('adapters/ticket-portal.ts')).toContain('SUPPORT_RD_TICKET_PORTAL_DECISION_REPLAY');
        expect(src('adapters/ticket-portal-sources.ts')).toContain('SUPPORT_RD_TICKET_PORTAL_VALID_BINDINGS');
        expect(src('adapters/ticket-portal-image-sql.ts')).not.toContain('portalImageAggregateSql');
        expect(src('adapters/support-incidents.ts')).toContain('const scopedWorkspaceListSql = (columns: string, table: string, extra: string) =>');
        expect(ticketRead).not.toContain('ticketEligibilitySelect');
        expect(evidence).toContain('const evidenceEligibilitySelect = () =>');
        expect(ticketRead).toContain('SELECT id FROM support_tickets WHERE id=? AND company_id=? FOR SHARE');
        expect(ticketRead).toContain('await runtime.requirePurchase("support.tickets")');
        expect(evidence).toContain('await runtime.requirePurchase("support.evidence")');
        expect(evidence).toContain('nativeSupportReadRuntime');
        expect(evidence).not.toMatch(/LOCK TABLE/);
    });

    test('retirement:ts-statements stays strictly below the r22 BEFORE=57 baseline', () => {
        let total = 0;
        const walk = (dir: string) => {
            for (const entry of readdirSync(dir, { withFileTypes: true })) {
                const path = resolve(dir, entry.name);
                if (entry.isDirectory()) walk(path);
                else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT/g) ?? []).length;
            }
        };
        walk(srcRoot);
        expect(total).toBeLessThan(57);
    });
});
