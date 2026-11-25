import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';

const srcRoot = resolve(__dirname, '../../../packages/server-support/src');
const incidents = readFileSync(resolve(srcRoot, 'adapters/support-incidents.ts'), 'utf8');
const src = (p: string) => readFileSync(resolve(srcRoot, p), 'utf8');

const scope = 't.company_id=? AND (?::text IS NULL OR t.branch_id=?) AND (?::text IS NULL OR t.department_id=?)';

/** Exact pre-dedupe (r21 BEFORE=58) prepared SQL from HEAD. The incident
 * trailing predicate is passed through untouched as `permission.sql`; only the
 * shared projection/table/order scaffold is factored into one helper. */
const beforeIncidents = `SELECT t.* FROM support_incidents t WHERE ${scope} AND <PERMISSION> ORDER BY t.updated_at DESC,t.id LIMIT 200`;
const beforeTickets = `SELECT t.id,t.title,t.version,t.status FROM support_tickets t WHERE ${scope} AND t.source_kind='sales_order' ORDER BY t.updated_at DESC,t.id LIMIT 200`;

describe('Support read-tail r21 workspace list family', () => {
    test('authors one parameterized workspace list projection', () => {
        expect(incidents.match(/const scopedWorkspaceListSql\s*=/g) ?? []).toHaveLength(1);
        const body = incidents.match(/const scopedWorkspaceListSql = \(columns: string, table: string, extra: string\) =>\s*readTailR43ColumnsFromSql\(columns, `([^`]*)`\);/)?.[1];
        expect(body).toBe('${table} t WHERE ${scope} AND ${extra} ORDER BY t.updated_at DESC,t.id LIMIT 200');
    });

    test('both call sites render byte-identically with unchanged binds', () => {
        const body = incidents.match(/const scopedWorkspaceListSql = \(columns: string, table: string, extra: string\) =>\s*readTailR43ColumnsFromSql\(columns, `([^`]*)`\);/)?.[1]!;
        const render = (columns: string, table: string, extra: string) => `SELECT ${columns} FROM ${body}`
            .replace('${columns}', columns)
            .replace('${table}', table)
            .replace('${scope}', scope)
            .replace('${extra}', extra);
        expect(render('t.*', 'support_incidents', '<PERMISSION>')).toBe(beforeIncidents);
        expect(render('t.id,t.title,t.version,t.status', 'support_tickets', "t.source_kind='sales_order'")).toBe(beforeTickets);
        expect(incidents).toContain("scopedWorkspaceListSql('t.*','support_incidents',permission.sql)).bind(...args(a), ...permission.bindings)");
        expect(incidents).toContain("scopedWorkspaceListSql('t.id,t.title,t.version,t.status','support_tickets',\"t.source_kind='sales_order'\")).bind(...args(a))");
        expect(incidents.match(/scopedWorkspaceListSql\(/g) ?? []).toHaveLength(2);
        expect(incidents).not.toContain('SELECT t.* FROM support_incidents t WHERE');
        expect(incidents).not.toContain("SELECT t.id,t.title,t.version,t.status FROM support_tickets t WHERE");
    });

    test('keeps the earlier helpers and the lock/purchase/native kinds intact', () => {
        expect(src('adapters/service-access.ts')).toContain('const serviceTeamSetSql = (table: string, fields: string, order: string) =>');
        expect(src('adapters/service-access.ts')).toContain('SUPPORT_RD_SERVICE_SCOPE_DEPARTMENT');
        expect(src('adapters/eligibility-projection-sources.ts')).toContain('export const supportAssignmentAccessSql = (alias: string, pad: string, tail: string) =>');
        expect(src('adapters/ticket-portal.ts')).toContain('SUPPORT_RD_TICKET_PORTAL_DECISION_REPLAY');
        expect(src('adapters/ticket-portal-sources.ts')).toContain('SUPPORT_RD_TICKET_PORTAL_VALID_BINDINGS');
        expect(src('adapters/ticket-portal-image-sql.ts')).not.toContain('portalImageAggregateSql');
        expect(incidents).toContain('await runtime.requirePurchase("support.incidents")');
        expect(incidents).toContain('await runtime.purchase("support.incidents")');
        expect(incidents).toContain('SUPPORT_RD_INCIDENT_TICKET_LINKS');
        expect(incidents).toContain('SUPPORT_RD_INCIDENT_EVENT_HISTORY');
        expect(incidents).toContain('SUPPORT_RD_INCIDENT_TICKET_CANDIDATES');
        expect(incidents).not.toMatch(/LOCK TABLE|FOR SHARE/);
    });

    test('retirement:ts-statements stays strictly below the r21 BEFORE=58 baseline', () => {
        let total = 0;
        const walk = (dir: string) => {
            for (const entry of readdirSync(dir, { withFileTypes: true })) {
                const path = resolve(dir, entry.name);
                if (entry.isDirectory()) walk(path);
                else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT/g) ?? []).length;
            }
        };
        walk(srcRoot);
        expect(total).toBeLessThan(58);
    });
});
