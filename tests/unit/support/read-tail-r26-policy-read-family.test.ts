import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test, vi } from 'vitest';
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';

vi.mock('server-only', () => ({}));

const srcRoot = resolve(__dirname, '../../../packages/server-support/src');
const src = (p: string) => readFileSync(resolve(srcRoot, p), 'utf8');
const sla = src('adapters/support-sla.ts');
const support = src('adapters/support.ts');
const bridge = src('adapters/native-support-read.ts');
const registry = src('adapters/native_support_read_sql.rs');

/** Exact r26 BEFORE=52 policy literals, now planned byte-for-byte in Rust. */
const beforePolicy = 'SELECT * FROM support_sla_policies WHERE id=? AND company_id=?';
const beforeApproved = "SELECT id,name,version FROM support_sla_policies WHERE company_id=? AND status='approved' ORDER BY name";

const config = JSON.stringify({
    privateBaseUrl: 'http://127.0.0.1:3997',
    timeoutMs: 1000,
    holdings: [{ origin: 'https://acme.jesadakorn.com', holdingSlug: 'acme', holdingId: 'h1', companyId: 'company' }],
});
const ok = (sql: string, values: unknown[]) =>
    new Response(JSON.stringify({ data: { statements: [{ sql, values }] } }), {
        status: 200,
        headers: { 'content-type': 'application/json' },
    });

describe('Support SLA policy native read port (r28)', () => {
    test('both policy reads are planned by two registered native kinds', () => {
        expect(registry).toContain(`"${beforePolicy}"`);
        expect(registry).toContain(`"${beforeApproved}"`);
        expect(registry).toContain('support_rd_sla_policy_point');
        expect(registry).toContain('support_rd_sla_policy_approved');
        expect(bridge).toContain("SUPPORT_RD_SLA_POLICY_POINT = 'support_rd_sla_policy_point'");
        expect(bridge).toContain("SUPPORT_RD_SLA_POLICY_APPROVED = 'support_rd_sla_policy_approved'");
        expect(bridge).toContain('[SUPPORT_RD_SLA_POLICY_POINT]: 2');
        expect(bridge).toContain('[SUPPORT_RD_SLA_POLICY_APPROVED]: 1');
    });

    test('both call sites route through the bridge and no SQL helper remains', () => {
        expect(sla).toContain('SUPPORT_RD_SLA_POLICY_POINT');
        expect(sla).not.toContain('supportPolicySelect');
        expect(support).toContain('SUPPORT_RD_SLA_POLICY_APPROVED');
        expect(support).not.toContain('supportPolicySelect');
        expect(() => src('adapters/support-policy-read-sources.ts')).toThrow();
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

    test('bridge plans point and approved policy reads on the caller unit', async () => {
        const batch = vi.fn(async () => [{ results: [{ id: 'policy-1' }], success: true, meta: { changes: 0, duration: 0 } }]);
        const db = { prepare: vi.fn(() => ({ bind: vi.fn() })), batch } as unknown as SqlDatabase;
        process.env.ERPX_NATIVE_SUPPORT_PLAN_CONFIG = config;
        const fetchMock = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => ok(beforePolicy, ['policy-1', 'company']));
        try {
            const { nativeSupportReadAll, SUPPORT_RD_SLA_POLICY_POINT, SUPPORT_RD_SLA_POLICY_APPROVED } =
                await import('@erpx/server-support/adapters/native-support-read');
            await expect(nativeSupportReadAll(SUPPORT_RD_SLA_POLICY_POINT, ['policy-1', 'company'], db)
                .then((result: { results: unknown[] }) => result.results)).resolves.toEqual([{ id: 'policy-1' }]);
            expect(JSON.parse(String(fetchMock.mock.calls[0]![1]!.body))).toMatchObject({
                kind: 'support_rd_sla_policy_point',
                values: ['policy-1', 'company'],
            });

            fetchMock.mockClear();
            await expect(nativeSupportReadAll(SUPPORT_RD_SLA_POLICY_APPROVED, ['company'], db)
                .then((result: { results: unknown[] }) => result.results)).resolves.toEqual([{ id: 'policy-1' }]);
            expect(JSON.parse(String(fetchMock.mock.calls[0]![1]!.body))).toMatchObject({
                kind: 'support_rd_sla_policy_approved',
                values: ['company'],
            });

            await expect(nativeSupportReadAll(SUPPORT_RD_SLA_POLICY_POINT, ['policy-1'], db)).rejects.toMatchObject({
                code: 'SUPPORT_NATIVE_READ_BINDS_INVALID',
            });
        } finally {
            fetchMock.mockRestore();
            delete process.env.ERPX_NATIVE_SUPPORT_PLAN_CONFIG;
        }
    });

    test('retirement:ts-statements stays strictly below the r28 BEFORE=52 baseline', () => {
        let total = 0;
        const walk = (dir: string) => {
            for (const entry of readdirSync(dir, { withFileTypes: true })) {
                const path = resolve(dir, entry.name);
                if (entry.isDirectory()) walk(path);
                else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT/g) ?? []).length;
            }
        };
        walk(srcRoot);
        expect(total).toBe(45);
        expect(total).toBeLessThan(50);
    });
});
