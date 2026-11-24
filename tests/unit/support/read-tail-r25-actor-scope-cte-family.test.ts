import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test, vi } from 'vitest';

vi.mock('server-only', () => ({}));
vi.mock('@erpx/server-identity_access/public/record-source-authority', () => ({
    currentAssignmentReadScope: () => ({ where: 'SCOPE_WHERE', args: ['branch', 'department'] }),
}));
vi.mock('@erpx/server-commercial/public/purchase-command-boundary', () => ({
    checkPurchasedCapability: async () => true,
    purchaseCommandFailure: () => null,
}));
vi.mock('@erpx/server-pms/public/feature-availability', () => ({
    scheduledFeaturePredicate: (column: string, feature: string) => `FEATURE(${column},${feature})`,
}));

import { actorScopeCteSql } from '../../../packages/server-support/src/adapters/actor-scope-read-sql';
import { workforceActorScopeCurrent } from '../../../packages/server-support/src/adapters/workforce-actor-scope-read';
import { ticketPortalCapability } from '../../../packages/server-support/src/adapters/ticket-portal-unit';

const srcRoot = resolve(__dirname, '../../../packages/server-support/src');
const src = (p: string) => readFileSync(resolve(srcRoot, p), 'utf8');
const shared = src('adapters/actor-scope-read-sql.ts');
const portal = src('adapters/ticket-portal-unit.ts');
const workforce = src('adapters/workforce-actor-scope-read.ts');

/** Exact pre-dedupe (r25 BEFORE=54) actor-scope CTE text from HEAD. Only the CTE
 * fragment moved into the helper; outer projection, binds and WHERE are unchanged. */
const beforePortalCte = 'SELECT ?::text company_id';
const beforeWorkforceCte = 'SELECT ?::text company_id,?::text branch_id,?::text department_id';
const beforePortalSql = 'SELECT FEATURE(portal_scope.company_id,support) allowed FROM (SELECT ?::text company_id) portal_scope';
const beforeWorkforceSql = 'SELECT 1 FROM(SELECT ?::text company_id,?::text branch_id,?::text department_id)workforce_actor_scope WHERE SCOPE_WHERE';

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

const actor = { companyId: 'company', branchId: 'branch', departmentId: 'department' } as never;

describe('Support read-tail r25 actor-scope CTE family', () => {
    test('one parameterized actor-scope CTE helper renders byte-identically', () => {
        expect(shared.match(/export const actorScopeCteSql\s*=/g) ?? []).toHaveLength(1);
        expect(actorScopeCteSql('')).toBe(beforePortalCte);
        expect(actorScopeCteSql(',?::text branch_id,?::text department_id')).toBe(beforeWorkforceCte);
    });

    test('call sites route through the helper with no inline CTE literal', () => {
        expect(portal.match(/actorScopeCteSql\(/g) ?? []).toHaveLength(1);
        expect(workforce.match(/actorScopeCteSql\(/g) ?? []).toHaveLength(1);
        expect(portal).not.toContain('(SELECT ?::text company_id)');
        expect(workforce).not.toContain('(SELECT ?::text company_id,?::text branch_id');
    });

    test('the portal feature probe renders byte-identically, binds unchanged', async () => {
        const { db, calls } = capture();
        await ticketPortalCapability(db, 'company', 'history');
        expect(calls).toHaveLength(1);
        expect(calls[0].sql).toBe(beforePortalSql);
        expect(calls[0].values).toEqual(['company']);
    });

    test('the workforce scope probe renders byte-identically, binds unchanged', async () => {
        const { db, calls } = capture();
        expect(await workforceActorScopeCurrent(db, actor)).toBe(true);
        expect(calls).toHaveLength(1);
        expect(calls[0].sql).toBe(beforeWorkforceSql);
        expect(calls[0].values).toEqual(['company', 'branch', 'department', 'branch', 'department']);
    });

    test('keeps locks, purchases, native readers and prior dedupe families intact', () => {
        expect(portal).toContain('SELECT pg_advisory_xact_lock(hashtextextended(?,0))');
        expect(portal).toContain('LOCK TABLE command_receipts,command_assertions,audit_events,outbox_events IN SHARE ROW EXCLUSIVE MODE');
        expect(src('adapters/attachment-read.ts')).toContain('LOCK TABLE support_tickets,support_ticket_events IN SHARE ROW EXCLUSIVE MODE');
        expect(src('adapters/support-ticket-read.ts')).toContain('SELECT id FROM support_tickets WHERE id=? AND company_id=? FOR SHARE');
        expect(workforce).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET/);
        expect(src('adapters/service-access.ts')).toContain('const activeAssignmentSql = (alias: string, col: string, gap: string, tail: string) =>');
        expect(src('adapters/ticket-portal-sources.ts')).toContain('export const portalReplayProofSql=');
        expect(src('adapters/ticket-portal-sources.ts')).toContain('export const portalValidBindingSelect=');
        expect(src('adapters/ticket-portal-image-sql.ts')).toContain('export const portalImageAggregateSql = (images: string[]) =>');
        expect(src('adapters/support-incidents.ts')).toContain('const scopedWorkspaceListSql = (columns: string, table: string, extra: string) =>');
        expect(src('adapters/ticket-guard-sources.ts')).toContain('export const ticketGuardExistsSql = (predicate: string) =>');
        expect(src('adapters/support-ticket-read.ts')).toContain('export const ticketEligibilitySelect =');
        expect(src('adapters/support-evidence.ts')).toContain('const evidenceEligibilitySelect = () =>');
        expect(src('adapters/executive-main-evidence-sources.ts')).toContain('const evidenceSelect = (columns: string) =>');
    });

    test('retirement:ts-statements stays strictly below the r25 BEFORE=54 baseline', () => {
        let total = 0;
        const walk = (dir: string) => {
            for (const entry of readdirSync(dir, { withFileTypes: true })) {
                const path = resolve(dir, entry.name);
                if (entry.isDirectory()) walk(path);
                else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT/g) ?? []).length;
            }
        };
        walk(srcRoot);
        expect(total).toBe(44);
        expect(total).toBeLessThan(50);
    });
});
