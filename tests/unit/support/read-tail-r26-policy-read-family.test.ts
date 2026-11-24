import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test, vi } from 'vitest';

vi.mock('server-only', () => ({}));

import { supportPolicySelect } from '../../../packages/server-support/src/adapters/support-policy-read-sources';

const srcRoot = resolve(__dirname, '../../../packages/server-support/src');
const src = (p: string) => readFileSync(resolve(srcRoot, p), 'utf8');
const shared = src('adapters/support-policy-read-sources.ts');
const sla = src('adapters/support-sla.ts');
const support = src('adapters/support.ts');

/** Exact pre-dedupe (r26 BEFORE=53) statements from HEAD. Only the fixed
 * table/columns/where/order renderings moved into one helper; binds, purchase
 * gates and every native reader remain untouched. */
const beforePolicy = 'SELECT * FROM support_sla_policies WHERE id=? AND company_id=?';
const beforeApproved = "SELECT id,name,version FROM support_sla_policies WHERE company_id=? AND status='approved' ORDER BY name";

describe('Support read-tail r26 SLA policy read family', () => {
    test('one parameterized policy-read helper renders byte-identically', () => {
        expect(shared.match(/export const supportPolicySelect\s*=/g) ?? []).toHaveLength(1);
        expect(supportPolicySelect('*', 'id=? AND company_id=?')).toBe(beforePolicy);
        expect(supportPolicySelect('id,name,version', "company_id=? AND status='approved'", ' ORDER BY name')).toBe(beforeApproved);
    });

    test('both call sites route through the helper with no inline statement', () => {
        expect(sla.match(/supportPolicySelect\(/g) ?? []).toHaveLength(1);
        expect(support.match(/supportPolicySelect\(/g) ?? []).toHaveLength(1);
        expect(sla).not.toContain(beforePolicy);
        expect(support).not.toContain(beforeApproved);
    });

    test('keeps locks, purchases, native readers and prior dedupe families intact', () => {
        expect(sla).toContain('requirePurchase("support.sla")');
        expect(support).toContain('requirePurchase("support.tickets")');
        expect(src('adapters/support-ticket-read.ts')).toContain('SELECT id FROM support_tickets WHERE id=? AND company_id=? FOR SHARE');
        expect(src('adapters/attachment-read.ts')).toContain('LOCK TABLE support_tickets,support_ticket_events IN SHARE ROW EXCLUSIVE MODE');
        expect(src('adapters/ticket-portal-unit.ts')).toContain('SELECT pg_advisory_xact_lock(hashtextextended(?,0))');
        expect(src('adapters/ticket-portal-unit.ts')).toContain('LOCK TABLE command_receipts,command_assertions,audit_events,outbox_events IN SHARE ROW EXCLUSIVE MODE');
        expect(src('adapters/service-access.ts')).toContain('const activeAssignmentSql = (alias: string, col: string, gap: string, tail: string) =>');
        expect(src('adapters/ticket-portal-sources.ts')).toContain('export const portalReplayProofSql=');
        expect(src('adapters/ticket-portal-sources.ts')).toContain('export const portalValidBindingSelect=');
        expect(src('adapters/ticket-portal-image-sql.ts')).toContain('export const portalImageAggregateSql = (images: string[]) =>');
        expect(src('adapters/support-incidents.ts')).toContain('const scopedWorkspaceListSql = (columns: string, table: string, extra: string) =>');
        expect(src('adapters/ticket-guard-sources.ts')).toContain('export const ticketGuardExistsSql = (predicate: string) =>');
        expect(src('adapters/support-ticket-read.ts')).toContain('export const ticketEligibilitySelect =');
        expect(src('adapters/support-evidence.ts')).toContain('const evidenceEligibilitySelect = () =>');
        expect(src('adapters/executive-main-evidence-sources.ts')).toContain('const evidenceSelect = (columns: string) =>');
        expect(src('adapters/actor-scope-read-sql.ts')).toContain('export const actorScopeCteSql = (extraColumns: string) =>');
    });

    test('retirement:ts-statements stays strictly below the r26 BEFORE=53 baseline', () => {
        let total = 0;
        const walk = (dir: string) => {
            for (const entry of readdirSync(dir, { withFileTypes: true })) {
                const path = resolve(dir, entry.name);
                if (entry.isDirectory()) walk(path);
                else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT/g) ?? []).length;
            }
        };
        walk(srcRoot);
        expect(total).toBe(52);
        expect(total).toBeLessThan(53);
    });
});
