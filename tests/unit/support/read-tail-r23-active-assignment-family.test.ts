import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import { finalRootOwnerProjectionSources } from '@erpx/server-identity_access/public/final-root-owner-projection-sources';
import { serviceMember, companyMember, serviceMemberSql } from '../../../packages/server-support/src/adapters/service-access';

const srcRoot = resolve(__dirname, '../../../packages/server-support/src');
const src = (p: string) => readFileSync(resolve(srcRoot, p), 'utf8');
const access = src('adapters/service-access.ts');
const assignments = finalRootOwnerProjectionSources.assignments;

/** Exact pre-dedupe (r23 BEFORE=56) prepared reads from HEAD. The scoped
 * service-membership probe and the company-membership probe shared one active
 * assignment read; only the alias, column qualifier, line gap and trailing
 * scope clause differ. One owner-local helper now renders both unchanged. */
const beforeMember = `EXISTS(SELECT 1 FROM (${assignments}) a WHERE a.company_id=? AND a.user_id=?
 AND a.revoked_at IS NULL AND a.valid_from<=(floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND a.valid_until>(floor(extract(epoch FROM clock_timestamp())*1000)::bigint)
 AND (a.scope_kind='holding' OR (a.scope_kind='branch' AND a.scope_node_id=?) OR (a.scope_kind='department' AND a.scope_node_id=?)))`;
const beforeCompany = `SELECT 1 FROM (${assignments}) assignments WHERE company_id=? AND user_id=? AND revoked_at IS NULL AND valid_from<=(floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND valid_until>(floor(extract(epoch FROM clock_timestamp())*1000)::bigint) LIMIT 1`;

type Call = { sql: string; values: unknown[] };
function capture() {
    const calls: Call[] = [];
    const db = {
        prepare(sql: string) {
            const call: Call = { sql, values: [] };
            calls.push(call);
            return {
                bind(...values: unknown[]) { call.values = values; return this; },
                async first() { return true; },
            };
        },
    } as unknown as import('@erpx/server-runtime/public/runtime/types').SqlDatabase;
    return { db, calls };
}

describe('Support read-tail r23 active-assignment family', () => {
    test('authors one parameterized active-assignment read helper', () => {
        expect(access.match(/const activeAssignmentSql\s*=/g) ?? []).toHaveLength(1);
        expect(access).toContain('const activeAssignmentSql = (alias: string, col: string, gap: string, tail: string) =>');
        expect(access.match(/SELECT 1 FROM \(\$\{identity_accessProjectionSources\.assignments\}\) \$\{alias\}/g) ?? []).toHaveLength(1);
        expect(serviceMemberSql).toBe(beforeMember);
        expect(access).not.toContain('`SELECT 1 FROM (${identity_accessProjectionSources.assignments}) assignments WHERE company_id=?');
    });

    test('both membership probes render byte-identically with unchanged binds', async () => {
        const scope = { company_id: 'company', branch_id: 'branch', department_id: 'department' };
        const member = capture();
        await serviceMember(member.db, scope as never, 'user');
        expect(member.calls).toHaveLength(1);
        expect(member.calls[0].sql).toBe(`SELECT 1 WHERE ${beforeMember}`);
        expect(member.calls[0].values).toEqual(['company', 'user', 'branch', 'department']);

        const company = capture();
        await companyMember(company.db, 'company', 'user');
        expect(company.calls).toHaveLength(1);
        expect(company.calls[0].sql).toBe(beforeCompany);
        expect(company.calls[0].values).toEqual(['company', 'user']);
    });

    test('keeps the earlier helpers and the lock/purchase/native kinds intact', () => {
        expect(access).toContain('const activeOrgNodeSql = (kind: string, branchMatch = \'\') =>');
        expect(access).toContain('const serviceTeamSetSql = (table: string, fields: string, order: string) =>');
        expect(src('adapters/eligibility-projection-sources.ts')).toContain('export const supportAssignmentAccessSql = (alias: string, pad: string, tail: string) =>');
        expect(src('adapters/ticket-portal-sources.ts')).toContain('export const portalReplayProofSql=');
        expect(src('adapters/ticket-portal-sources.ts')).toContain('export const portalValidBindingSelect=');
        expect(src('adapters/ticket-portal-image-sql.ts')).toContain('export const portalImageAggregateSql = (images: string[]) =>');
        expect(src('adapters/support-incidents.ts')).toContain('const scopedWorkspaceListSql = (columns: string, table: string, extra: string) =>');
        expect(src('adapters/ticket-guard-sources.ts')).toContain('export const ticketGuardExistsSql = (predicate: string) =>');
        expect(src('adapters/support-ticket-read.ts')).toContain('export const ticketEligibilitySelect =');
        expect(src('adapters/support-evidence.ts')).toContain('const evidenceEligibilitySelect = () =>');
        expect(src('adapters/support-ticket-read.ts')).toContain('await runtime.requirePurchase("support.tickets")');
        expect(src('adapters/ticket-portal-unit.ts')).toContain('SELECT pg_advisory_xact_lock(hashtextextended(?,0))');
        expect(src('adapters/attachment-read.ts')).toContain('LOCK TABLE support_tickets,support_ticket_events IN SHARE ROW EXCLUSIVE MODE');
    });

    test('retirement:ts-statements stays strictly below the r23 BEFORE=56 baseline', () => {
        let total = 0;
        const walk = (dir: string) => {
            for (const entry of readdirSync(dir, { withFileTypes: true })) {
                const path = resolve(dir, entry.name);
                if (entry.isDirectory()) walk(path);
                else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT/g) ?? []).length;
            }
        };
        walk(srcRoot);
        expect(total).toBe(55);
        expect(total).toBeLessThan(56);
    });
});
