import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it, vi } from 'vitest';
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';

vi.mock('server-only', () => ({}));
vi.mock('@erpx/server-support/adapters/service-access', () => ({ serviceMember: async () => true }));
vi.mock('@erpx/server-identity_access/public/iam-person', () => ({
    currentPersonPrincipals: async () => new Map(),
}));

const srcRoot = resolve(__dirname, '../../../packages/server-support/src');
const src = (f: string) => readFileSync(resolve(srcRoot, f), 'utf8');
const consumer = src('adapters/service-team-reads.ts');
const bridge = src('adapters/native-support-read.ts');
const registry = src('adapters/native_support_read_sql.rs');
const projectionSource = readFileSync(
    resolve(__dirname, '../../../packages/server-people_payroll/src/adapters/pp-read-tail-r18-sources.ts'),
    'utf8',
);

const kind = 'support_rd_service_team_members_named';
const projection = JSON.parse(projectionSource.match(/PP_RD18_HR_PEOPLE_PROJ_SQL = ("(?:[^"\\]|\\.)*")/)![1]) as string;
const retiredSelect = `SELECT m.user_id,m.responsibility,COALESCE(p.legal_name,m.user_id) name FROM service_team_members m JOIN service_teams t ON t.id=m.team_id AND t.company_id=m.company_id LEFT JOIN (${projection}) p ON p.company_id=m.company_id AND p.user_id=m.user_id AND p.merged_into_id IS NULL WHERE t.id=? AND t.company_id=? AND t.active=1 ORDER BY m.user_id`;
const rustLiteral = retiredSelect.replaceAll('"', '\\"');
const orgSource = readFileSync(
    resolve(__dirname, '../../../packages/server-organization/src/adapters/org-sources.ts'),
    'utf8',
);
const orgNodes = 'SELECT "id","kind","slug","display_name","parent_id","company_id","branch_id","status","hierarchy_revision" FROM organization_nodes';
const workTeamRetired = `SELECT 1 FROM service_team_members m
    JOIN service_teams t ON t.id=m.team_id AND t.company_id=m.company_id AND t.active=1
    JOIN service_team_coverage c ON c.team_id=t.id AND c.company_id=t.company_id
    JOIN (${orgNodes}) n ON n.id=c.scope_node_id AND n.company_id=c.company_id AND n.status='active'
    LEFT JOIN (${orgNodes}) b ON b.id=n.branch_id AND b.company_id=n.company_id AND b.kind='branch'
    WHERE m.company_id=? AND m.user_id=? AND (
      (c.scope_kind='holding' AND n.kind='company' AND n.id=m.company_id) OR
      (c.scope_kind='branch' AND n.kind='branch' AND (CAST(? AS TEXT) IS NULL OR n.id=?)) OR
      (c.scope_kind='department' AND n.kind='department' AND (n.branch_id IS NULL OR b.status='active') AND (CAST(? AS TEXT) IS NULL OR n.branch_id=?) AND (CAST(? AS TEXT) IS NULL OR n.id=?))
    ) LIMIT 1`;
const workTeamRustLiteral = workTeamRetired.replaceAll('"', '\\"').replaceAll('\n', '\\n');

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

describe('Support read-tail r37 service-team members named kind', () => {
    it('names the kind and its fixed 2-bind arity in the bridge and the Rust registry', () => {
        expect(bridge).toContain(`SUPPORT_RD_SERVICE_TEAM_MEMBERS_NAMED = '${kind}'`);
        expect(bridge).toContain('[SUPPORT_RD_SERVICE_TEAM_MEMBERS_NAMED]: 2,');
        expect(registry).toContain(`RD_SERVICE_TEAM_MEMBERS_NAMED: &str = "${kind}"`);
        expect(registry).toContain('kind: RD_SERVICE_TEAM_MEMBERS_NAMED,');
        expect(registry).toContain('    READ_SERVICE_TEAM_MEMBERS_NAMED,\n');
        expect(registry).toContain('RD_SERVICE_TEAM_MEMBERS_NAMED, SQL_SERVICE_TEAM_MEMBERS_NAMED, 2usize');
        expect(retiredSelect.match(/\?/g) ?? []).toHaveLength(2);
    });

    it('carries the retired owner literal byte-for-byte into the registry and leaves no local SELECT', () => {
        expect(registry).toContain(`"${rustLiteral}"`);
        expect(registry).toContain('service_team_members_named_read_renders_the_retired_owner_literal_byte_for_byte');
        expect(consumer).not.toContain('SELECT');
        expect(consumer).not.toContain('finalRootOwnerProjectionSources');
        expect(consumer).toContain('SUPPORT_RD_SERVICE_TEAM_MEMBERS_NAMED');
        expect(retiredSelect).toContain('COALESCE(p.legal_name,m.user_id) name');
        expect(retiredSelect).toContain('WHERE t.id=? AND t.company_id=? AND t.active=1 ORDER BY m.user_id');
    });

    it('plans the read on the caller unit with the team id leading the two binds', async () => {
        const batch = vi.fn(async () => [{
            results: [{ user_id: 'u1', responsibility: 'lead', name: 'Support Lead' }],
            success: true,
            meta: { changes: 0, duration: 0 },
        }]);
        const db = { prepare: vi.fn(() => ({ bind: vi.fn() })), batch } as unknown as SqlDatabase;
        process.env.ERPX_NATIVE_SUPPORT_PLAN_CONFIG = config;
        const fetchMock = vi.spyOn(globalThis, 'fetch').mockResolvedValue(ok(retiredSelect, ['team-1', 'company']));
        try {
            const { teamHandlers } = await import('@erpx/server-support/adapters/service-team-reads');
            const scope = { company_id: 'company', branch_id: null, department_id: null } as never;
            await expect(teamHandlers(db, scope, 'team-1')).resolves.toEqual([
                { user_id: 'u1', responsibility: 'lead', name: 'Support Lead' },
            ]);
            expect(String(fetchMock.mock.calls[0]![0])).toContain('/support/command-plan');
            expect(JSON.parse(String(fetchMock.mock.calls[0]![1]!.body))).toMatchObject({
                kind,
                values: ['team-1', 'company'],
            });
            expect(db.prepare).toHaveBeenCalledWith(retiredSelect);
        } finally {
            fetchMock.mockRestore();
            delete process.env.ERPX_NATIVE_SUPPORT_PLAN_CONFIG;
        }
    });

    it('fails closed on a wrong bind arity before transport', async () => {
        process.env.ERPX_NATIVE_SUPPORT_PLAN_CONFIG = config;
        const fetchMock = vi.spyOn(globalThis, 'fetch').mockResolvedValue(ok('SELECT 1', []));
        const db = { prepare: vi.fn(() => ({ bind: vi.fn() })), batch: vi.fn() } as unknown as SqlDatabase;
        try {
            const { nativeSupportReadAll, SUPPORT_RD_SERVICE_TEAM_MEMBERS_NAMED } =
                await import('@erpx/server-support/adapters/native-support-read');
            await expect(nativeSupportReadAll(SUPPORT_RD_SERVICE_TEAM_MEMBERS_NAMED, ['team-1'], db))
                .rejects.toMatchObject({ status: 422, code: 'SUPPORT_NATIVE_READ_BINDS_INVALID' });
            await expect(nativeSupportReadAll(kind, ['team-1', 'company', 'extra'], db))
                .rejects.toMatchObject({ status: 422, code: 'SUPPORT_NATIVE_READ_BINDS_INVALID' });
            expect(fetchMock).not.toHaveBeenCalled();
        } finally {
            fetchMock.mockRestore();
            delete process.env.ERPX_NATIVE_SUPPORT_PLAN_CONFIG;
        }
    });

    it('names the work-team coverage kind with its fixed 8-bind arity', () => {
        const workTeam = src('adapters/work-team-coverage-facts.ts');
        expect(bridge).toContain("SUPPORT_RD_WORK_TEAM_COVERAGE = 'support_rd_work_team_coverage'");
        expect(bridge).toContain('[SUPPORT_RD_WORK_TEAM_COVERAGE]: 8,');
        expect(registry).toContain('RD_WORK_TEAM_COVERAGE: &str = "support_rd_work_team_coverage"');
        expect(registry).toContain('kind: RD_WORK_TEAM_COVERAGE,');
        expect(registry).toContain('    READ_WORK_TEAM_COVERAGE,\n');
        expect(registry).toContain('(RD_WORK_TEAM_COVERAGE, SQL_WORK_TEAM_COVERAGE, 8usize),');
        expect(workTeam).not.toContain('SELECT');
        expect(workTeam).not.toContain('organizationProjectionSources');
        expect(workTeam).toContain('SUPPORT_RD_WORK_TEAM_COVERAGE');
        expect(workTeamRetired.match(/\?/g) ?? []).toHaveLength(8);
    });

    it('carries the work-team coverage literal byte-for-byte over the closed node projection', () => {
        expect(orgSource).toContain('"id","kind","slug","display_name","parent_id","company_id","branch_id","status","hierarchy_revision"');
        expect(orgSource).toContain('organization_nodes');
        expect(registry).toContain(`"${workTeamRustLiteral}"`);
        expect(registry).toContain('work_team_coverage_read_renders_the_retired_owner_literal_byte_for_byte');
        expect(workTeamRetired).toContain('SELECT 1 FROM service_team_members m');
        expect(workTeamRetired).toContain('LIMIT 1');
    });

    it('plans work-team coverage with the company/user id leading the eight binds', async () => {
        const values = ['company', 'user', 'branch', 'branch', 'branch', 'branch', 'dept', 'dept'];
        const batch = vi.fn(async () => [{ results: [{ '1': 1 }], success: true, meta: { changes: 0, duration: 0 } }]);
        const db = { prepare: vi.fn(() => ({ bind: vi.fn() })), batch } as unknown as SqlDatabase;
        process.env.ERPX_NATIVE_SUPPORT_PLAN_CONFIG = config;
        const fetchMock = vi.spyOn(globalThis, 'fetch').mockResolvedValue(ok(workTeamRetired, values));
        try {
            const { workTeamCoverageFacts } = await import('@erpx/server-support/adapters/work-team-coverage-facts');
            const actor = { companyId: 'company', userId: 'user', branchId: 'branch', departmentId: 'dept' } as never;
            await expect(workTeamCoverageFacts(db).coveredTeam(actor)).resolves.toEqual({ '1': 1 });
            expect(String(fetchMock.mock.calls[0]![0])).toContain('/support/command-plan');
            expect(JSON.parse(String(fetchMock.mock.calls[0]![1]!.body))).toMatchObject({
                kind: 'support_rd_work_team_coverage',
                values,
            });
            expect(db.prepare).toHaveBeenCalledWith(workTeamRetired);
        } finally {
            fetchMock.mockRestore();
            delete process.env.ERPX_NATIVE_SUPPORT_PLAN_CONFIG;
        }
    });

    it('pins the KPI: 46->50 registry kinds and 37->34 owner SELECT tokens', () => {
        expect(registry).toContain('assert_eq!(STATEMENTS.len(), 50);');
        expect(selectCount()).toBe(34);
        expect(selectCount()).toBeLessThan(42);
    });

    it('leaves locks, advisory and other registered kinds untouched', () => {
        expect(registry).not.toMatch(/LOCK TABLE|pg_advisory_xact_lock|workflow_command_assertion/);
        for (const kept of [
            'SUPPORT_RD_TICKET_RECIPIENTS',
            'SUPPORT_RD_SLA_ALERTS',
            'SUPPORT_RD_CUSTOMER_CONTACT_NOTES',
            'SUPPORT_RD_SERVICE_TEAM_COVERAGE',
        ]) expect(bridge).toContain(kept);
        for (const kept of [
            'READ_TICKET_RECIPIENTS',
            'READ_SLA_ALERTS',
            'READ_CUSTOMER_CONTACT_NOTES',
            'READ_SERVICE_TEAM_COVERAGE',
        ]) expect(registry).toContain(`    ${kept},\n`);
    });
});
