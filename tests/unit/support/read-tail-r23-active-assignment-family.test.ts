import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test, vi } from 'vitest';

const { calls } = vi.hoisted(() => ({
    calls: [] as Array<{ kind: string; values: readonly unknown[] }>,
}));

vi.mock('server-only', () => ({}));
vi.mock('@erpx/server-support/adapters/native-support-read', async (importOriginal) => {
    const original = await importOriginal<typeof import('../../../packages/server-support/src/adapters/native-support-read')>();
    return {
        ...original,
        nativeSupportReadAll: async (kind: string, values: readonly unknown[]) => {
            calls.push({ kind, values });
            return { results: [{ ok: 1 }], success: true, meta: { changes: 0, duration: 0 } };
        },
    };
});

import { serviceMember, companyMember } from '../../../packages/server-support/src/adapters/service-access';
import {
    SUPPORT_RD_SERVICE_MEMBER,
    SUPPORT_RD_COMPANY_MEMBER,
} from '../../../packages/server-support/src/adapters/native-support-read';
import { finalRootOwnerProjectionSources } from '@erpx/server-identity_access/public/final-root-owner-projection-sources';

const srcRoot = resolve(__dirname, '../../../packages/server-support/src');
const src = (p: string) => readFileSync(resolve(srcRoot, p), 'utf8');
const access = src('adapters/service-access.ts');
const registry = src('adapters/native_support_read_sql.rs');
const bridge = src('adapters/native-support-read.ts');
const assignments = finalRootOwnerProjectionSources.assignments;

/** Exact pre-r23 (r23 BEFORE=56) prepared reads from HEAD. The native registry
 * now carries both byte-identical literals; the TypeScript renderer is gone. */
const beforeMember = `SELECT 1 WHERE EXISTS(SELECT 1 FROM (${assignments}) a WHERE a.company_id=? AND a.user_id=?
 AND a.revoked_at IS NULL AND a.valid_from<=(floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND a.valid_until>(floor(extract(epoch FROM clock_timestamp())*1000)::bigint)
 AND (a.scope_kind='holding' OR (a.scope_kind='branch' AND a.scope_node_id=?) OR (a.scope_kind='department' AND a.scope_node_id=?)))`;
const beforeCompany = `SELECT 1 FROM (${assignments}) assignments WHERE company_id=? AND user_id=? AND revoked_at IS NULL AND valid_from<=(floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND valid_until>(floor(extract(epoch FROM clock_timestamp())*1000)::bigint) LIMIT 1`;

/** Decode the Rust `&str` literal that the command-plan registry serves. */
function rustLiteral(name: string): string {
    const start = registry.indexOf(`const ${name}: &str =`);
    expect(start).toBeGreaterThan(-1);
    const open = registry.indexOf('"', start);
    const close = registry.indexOf('";', open);
    expect(open).toBeGreaterThan(-1);
    expect(close).toBeGreaterThan(open);
    return JSON.parse(registry.slice(open, close + 1)) as string;
}

describe('Support read-tail r23 active-assignment family', () => {
    test('routes the scoped service and company membership probes through the native owner', async () => {
        calls.length = 0;
        const db = {} as never;
        const scope = { company_id: 'company', branch_id: 'branch', department_id: 'department' };
        expect(await serviceMember(db, scope as never, 'user')).toBe(true);
        expect(await companyMember(db, 'company', 'user')).toBe(true);
        expect(calls).toEqual([
            { kind: SUPPORT_RD_SERVICE_MEMBER, values: ['company', 'user', 'branch', 'department'] },
            { kind: SUPPORT_RD_COMPANY_MEMBER, values: ['company', 'user'] },
        ]);
        expect(SUPPORT_RD_SERVICE_MEMBER).toBe('support_rd_service_member');
        expect(SUPPORT_RD_COMPANY_MEMBER).toBe('support_rd_company_member');
    });

    test('deletes the TypeScript renderer and keeps both native literals byte-identical', () => {
        expect(access).not.toContain('SELECT 1 FROM (${identity_accessProjectionSources.assignments})');
        expect(access).not.toContain('SELECT 1 WHERE ${serviceMemberSql}');
        expect(access).not.toContain('activeAssignmentSql');
        expect(access).not.toContain('serviceMemberSql');
        expect(access).not.toContain('identity_accessProjectionSources');
        expect(access).toContain('SUPPORT_RD_SERVICE_MEMBER');
        expect(access).toContain('SUPPORT_RD_COMPANY_MEMBER');
        expect(rustLiteral('SQL_SERVICE_MEMBER')).toBe(beforeMember);
        expect(rustLiteral('SQL_COMPANY_MEMBER')).toBe(beforeCompany);
        expect(beforeMember.match(/\?/g) ?? []).toHaveLength(4);
        expect(beforeCompany.match(/\?/g) ?? []).toHaveLength(2);
        expect(registry).toContain('pub const RD_SERVICE_MEMBER: &str = "support_rd_service_member"');
        expect(registry).toContain('pub const RD_COMPANY_MEMBER: &str = "support_rd_company_member"');
        expect(registry).toContain('(RD_SERVICE_MEMBER, SQL_SERVICE_MEMBER, 4usize)');
        expect(registry).toContain('(RD_COMPANY_MEMBER, SQL_COMPANY_MEMBER, 2usize)');
        expect(bridge).toContain("SUPPORT_RD_SERVICE_MEMBER = 'support_rd_service_member'");
        expect(bridge).toContain("SUPPORT_RD_COMPANY_MEMBER = 'support_rd_company_member'");
        expect(src('public/service-access.ts')).not.toContain('serviceMemberSql');
    });

    test('keeps the earlier helpers and the lock/purchase/native kinds intact', () => {
        expect(access).toContain('SUPPORT_RD_SERVICE_SCOPE_BRANCH');
        expect(access).toContain('const serviceTeamSetSql = (table: string, fields: string, order: string) =>');
        expect(src('adapters/eligibility-projection-sources.ts')).toContain('export const supportAssignmentAccessSql = (alias: string, pad: string, tail: string) =>');
        expect(src('adapters/ticket-portal.ts')).toContain('SUPPORT_RD_TICKET_PORTAL_DECISION_REPLAY');
        expect(src('adapters/ticket-portal-sources.ts')).toContain('SUPPORT_RD_TICKET_PORTAL_VALID_BINDINGS');
        expect(src('adapters/ticket-portal-image-sql.ts')).not.toContain('portalImageAggregateSql');
        expect(src('adapters/support-incidents.ts')).toContain('const scopedWorkspaceListSql = (columns: string, table: string, extra: string) =>');
        expect(src('adapters/ticket-guard-sources.ts')).toContain('export const ticketGuardExistsSql = (predicate: string) =>');
        expect(src('adapters/support-ticket-read.ts')).not.toContain('ticketEligibilitySelect');
        expect(src('adapters/support-evidence.ts')).toContain('const evidenceEligibilitySelect = () =>');
        expect(src('adapters/support-ticket-read.ts')).toContain('await runtime.requirePurchase("support.tickets")');
        expect(src('adapters/ticket-portal-unit.ts')).toContain('SELECT pg_advisory_xact_lock(hashtextextended(?,0))');
        expect(src('adapters/attachment-read.ts')).toContain('LOCK TABLE support_tickets,support_ticket_events IN SHARE ROW EXCLUSIVE MODE');
    });

    test('retirement:ts-statements drops from the r46 AFTER=30 to 20', () => {
        let total = 0;
        const walk = (dir: string) => {
            for (const entry of readdirSync(dir, { withFileTypes: true })) {
                const path = resolve(dir, entry.name);
                if (entry.isDirectory()) walk(path);
                else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
            }
        };
        walk(srcRoot);
        expect(total).toBe(20);
        expect(total).toBeLessThan(30);
    });
});
