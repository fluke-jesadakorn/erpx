import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test, vi } from 'vitest';

vi.mock('server-only', () => ({}));
vi.mock('@erpx/server-support/adapters/command-runtime', () => ({
    nativeSupportRuntime: () => ({ purchase: async () => ({ history: true }), requirePurchase: async () => ({ history: true }) }),
}));
vi.mock('@erpx/server-support/adapters/ticket-source-read-policy', () => ({
    supportTicketSourceReadPredicate: async () => ({ sql: 'SOURCE_PREDICATE', bindings: ['src'] }),
}));

import { executiveMainEvidenceSources } from '../../../packages/server-support/src/adapters/executive-main-evidence-sources';
import { supportTicketReadPredicate } from '../../../packages/server-support/src/adapters/support-ticket-read';
import { supportIncidentReadPredicate } from '../../../packages/server-support/src/adapters/support-incident-read';

const srcRoot = resolve(__dirname, '../../../packages/server-support/src');
const src = (p: string) => readFileSync(resolve(srcRoot, p), 'utf8');
const evidence = src('adapters/executive-main-evidence-sources.ts');
const actor = {
    companyId: 'company', branchId: 'branch', departmentId: 'department',
    assignmentId: 'assignment', role: 'support.read', userId: 'user',
} as never;

/** Exact pre-dedupe (r24 BEFORE=55) prepared statements from HEAD. Only the
 * projection prefix differed; FROM/WHERE/ORDER text and binds stay unchanged. */
const incidentColumns = 't.id,t.title,t.severity,t.status,t.due_date,t.created_at,t.updated_at,t.version';
const ticketColumns = (overdue: boolean) =>
    `t.id,t.title,t.status,t.due_date,t.updated_at,t.version${overdue ? ",t.sla_clock_json->>'state' sla_state,t.sla_clock_json->>'dueAt' sla_due_at" : ''}`;
const incidentPolicy = supportIncidentReadPredicate(actor).sql;
const ticketPolicy = supportTicketReadPredicate(actor).sql;
const scope = 't.company_id=? AND (?::text IS NULL OR t.branch_id=?) AND (?::text IS NULL OR t.department_id=?)';
const overdueSuffix = ` AND t.sla_clock_json->>'state'='running' AND CASE
    WHEN t.sla_clock_json->>'dueAt' ~ '^[0-9]+$'
      THEN (t.sla_clock_json->>'dueAt')::numeric < floor(extract(epoch FROM clock_timestamp())*1000)::numeric
    ELSE FALSE END`;
const beforeIncident = `SELECT ${incidentColumns}
    FROM support_incidents t WHERE ${scope} AND ${incidentPolicy} AND t.status IN ('open','investigating','mitigated') ORDER BY t.updated_at DESC,t.id ASC LIMIT ?`;
const beforeTicketSearch = (overdue: boolean) => `SELECT ${ticketColumns(overdue)}
    FROM support_tickets t WHERE ${scope} AND ${ticketPolicy} AND SOURCE_PREDICATE AND t.status NOT IN ('closed','cancelled')${overdue ? overdueSuffix : ''} ORDER BY t.updated_at DESC,t.id ASC LIMIT ?`;
const beforeTicketById = `SELECT ${ticketColumns(false)}
    FROM support_tickets t WHERE t.id=? AND t.company_id=? AND (?::text IS NULL OR t.branch_id=?) AND (?::text IS NULL OR t.department_id=?) AND ${ticketPolicy} AND SOURCE_PREDICATE AND t.status NOT IN ('closed','cancelled') LIMIT 1`;

type Call = { sql: string; values: unknown[] };
function capture() {
    const calls: Call[] = [];
    const db = {
        prepare(sql: string) {
            const call: Call = { sql, values: [] };
            calls.push(call);
            return {
                bind(...values: unknown[]) { call.values = values; return this; },
                async all() { return { results: [] }; },
                async first() { return null; },
            };
        },
    } as unknown as import('@erpx/server-runtime/public/runtime/types').SqlDatabase;
    return { db, calls, sources: executiveMainEvidenceSources(db) };
}
const input = (metric: 'active-support-incidents' | 'open-support-tickets' | 'overdue-support-sla') =>
    ({ filters: { from: '0', to: '1', currency: 'THB' }, query: '', limit: 3, metric });

describe('Support read-tail r24 evidence projection family', () => {
    test('authors one parameterized evidence projection helper', () => {
        expect(evidence.match(/const evidenceSelect\s*=/g) ?? []).toHaveLength(1);
        expect(evidence).toContain('const evidenceSelect = (columns: string) => `SELECT ${columns}`;');
        expect(evidence).not.toContain('const ticketEvidenceSelect =');
        expect(evidence).not.toContain('`SELECT t.id,t.title,t.severity');
        expect(evidence.match(/evidenceSelect\(/g) ?? []).toHaveLength(3);
    });

    test('incident search renders byte-identically to its pre-dedupe statement', async () => {
        const { sources, calls } = capture();
        await sources.factsSearchSupportIncidentEvidence(null, input('active-support-incidents') as never, actor);
        expect(calls).toHaveLength(1);
        expect(calls[0].sql).toBe(beforeIncident);
        expect(calls[0].values).toEqual(['company', 'branch', 'branch', 'department', 'department', 'assignment', 'user', 'support.read', 4]);
    });

    test('ticket search renders byte-identically, including overdue columns', async () => {
        for (const overdue of [false, true]) {
            const { sources, calls } = capture();
            await sources.factsSearchSupportTicketEvidence(
                input(overdue ? 'overdue-support-sla' : 'open-support-tickets') as never, null, overdue, actor,
            );
            expect(calls).toHaveLength(1);
            expect(calls[0].sql).toBe(beforeTicketSearch(overdue));
            expect(calls[0].values).toEqual([
                'company', 'branch', 'branch', 'department', 'department', 'assignment', 'user', 'support.read', 'src', 4,
            ]);
        }
    });

    test('ticket by-id reader renders byte-identically', async () => {
        const { sources, calls } = capture();
        await sources.factsReadSupportTicketEvidence(false, 'open-support-tickets', 'ticket-1', actor);
        expect(calls).toHaveLength(1);
        expect(calls[0].sql).toBe(beforeTicketById);
        expect(calls[0].values).toEqual([
            'ticket-1', 'company', 'branch', 'branch', 'department', 'department', 'assignment', 'user', 'support.read', 'src',
        ]);
    });

    test('keeps locks, purchases and prior dedupe families intact', () => {
        expect(src('adapters/service-access.ts')).toContain('const activeAssignmentSql = (alias: string, col: string, gap: string, tail: string) =>');
        expect(src('adapters/ticket-portal-sources.ts')).toContain('export const portalReplayProofSql=');
        expect(src('adapters/ticket-portal-sources.ts')).toContain('export const portalValidBindingSelect=');
        expect(src('adapters/ticket-portal-image-sql.ts')).toContain('export const portalImageAggregateSql = (images: string[]) =>');
        expect(src('adapters/support-incidents.ts')).toContain('const scopedWorkspaceListSql = (columns: string, table: string, extra: string) =>');
        expect(src('adapters/ticket-guard-sources.ts')).toContain('export const ticketGuardExistsSql = (predicate: string) =>');
        expect(src('adapters/support-ticket-read.ts')).toContain('export const ticketEligibilitySelect =');
        expect(src('adapters/support-evidence.ts')).toContain('const evidenceEligibilitySelect = () =>');
        expect(evidence).toContain('nativeSupportRuntime(');
        expect(src('adapters/ticket-portal-unit.ts')).toContain('SELECT pg_advisory_xact_lock(hashtextextended(?,0))');
        expect(src('adapters/attachment-read.ts')).toContain('LOCK TABLE support_tickets,support_ticket_events IN SHARE ROW EXCLUSIVE MODE');
    });

    test('retirement:ts-statements stays strictly below the r24 BEFORE=55 baseline', () => {
        let total = 0;
        const walk = (dir: string) => {
            for (const entry of readdirSync(dir, { withFileTypes: true })) {
                const path = resolve(dir, entry.name);
                if (entry.isDirectory()) walk(path);
                else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT/g) ?? []).length;
            }
        };
        walk(srcRoot);
        expect(total).toBe(48);
        expect(total).toBeLessThan(50);
    });
});
