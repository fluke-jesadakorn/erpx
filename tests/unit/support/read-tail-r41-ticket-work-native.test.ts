import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test, vi } from 'vitest';

vi.mock('server-only', () => ({}));

import { supportTicketReadPredicate } from '../../../packages/server-support/src/adapters/support-ticket-read';
import { args } from '../../../packages/server-support/src/adapters/support-ticket-read-sources';

const srcRoot = resolve(__dirname, '../../../packages/server-support/src');
const src = (p: string) => readFileSync(resolve(srcRoot, p), 'utf8');
const registry = src('adapters/native_support_read_sql.rs');
const bridge = src('adapters/native-support-read.ts');
const workSource = src('adapters/support.ts');

/** Rust string literal -> exact SQL text (only double quotes and newlines are escaped). */
const rustLiteral = (name: string): string => {
    const match = registry.match(new RegExp(`const SQL_${name}: &str =\\s*("(?:[^"\\\\]|\\\\.)*");`));
    if (!match) throw new Error(`missing SQL_${name}`);
    return JSON.parse(match[1]!) as string;
};

/** Byte oracle captured from support.ts:25 before the cutover (HEAD render of the shared helpers). */
const goldenWork = "SELECT t.*,CASE WHEN EXISTS(SELECT 1 FROM (SELECT \"id\",\"user_id\",\"role\",\"modules_json\",\"company_id\",\"branch_id\",\"valid_from\",\"valid_until\",\"revoked_at\",\"version\",\"scope_kind\",\"scope_node_id\",\"parent_id\",\"source\",\"source_id\" FROM assignments) a JOIN (SELECT \"id\",\"user_id\",\"company_id\",\"legal_name\",\"preferred_name\",\"email\",\"phone\",\"country_code\",\"status\",\"version\",\"created_by\",\"created_at\",\"updated_at\",\"merged_into_id\",\"work_email\",\"work_phone\" FROM hr_people) p ON p.user_id=a.user_id AND p.company_id=a.company_id JOIN (SELECT \"id\",\"person_id\",\"company_id\",\"branch_id\",\"department_id\",\"worker_type\",\"employment_type\",\"status\",\"start_date\",\"end_date\",\"manager_person_id\",\"job_title\",\"currency\",\"version\",\"created_by\",\"created_at\",\"updated_at\",\"pay_group_id\",\"level_id\",\"position_id\",\"self_service_top_level\" FROM hr_engagements) e ON e.person_id=p.id AND e.company_id=p.company_id WHERE a.company_id=t.company_id AND a.user_id=t.owner_id AND iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(a.user_id,a.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(a.role,a.company_id,'support.work') AND iam_role_allows(a.role,a.company_id,'support.read') AND (a.scope_kind='holding' OR (a.scope_kind='branch' AND a.scope_node_id=t.branch_id) OR (a.scope_kind='department' AND a.scope_node_id=t.department_id)) AND p.status='active' AND p.merged_into_id IS NULL AND e.status='active' AND e.start_date<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') AND (e.end_date IS NULL OR e.end_date>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD'))) THEN 1 ELSE 0 END owner_eligible,(SELECT COUNT(DISTINCT m.user_id) FROM (SELECT \"id\",\"user_id\",\"role\",\"modules_json\",\"company_id\",\"branch_id\",\"valid_from\",\"valid_until\",\"revoked_at\",\"version\",\"scope_kind\",\"scope_node_id\",\"parent_id\",\"source\",\"source_id\" FROM assignments) m WHERE m.company_id=t.company_id AND m.user_id<>t.owner_id AND m.user_id IS DISTINCT FROM t.resolved_by AND iam_assignment_active(m.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(m.role,m.company_id,'support.manage') AND (m.scope_kind='holding' OR (m.scope_kind='branch' AND m.scope_node_id=t.branch_id) OR (m.scope_kind='department' AND m.scope_node_id=t.department_id)) AND EXISTS(SELECT 1 FROM (SELECT \"id\",\"user_id\",\"role\",\"modules_json\",\"company_id\",\"branch_id\",\"valid_from\",\"valid_until\",\"revoked_at\",\"version\",\"scope_kind\",\"scope_node_id\",\"parent_id\",\"source\",\"source_id\" FROM assignments) a JOIN (SELECT \"id\",\"user_id\",\"company_id\",\"legal_name\",\"preferred_name\",\"email\",\"phone\",\"country_code\",\"status\",\"version\",\"created_by\",\"created_at\",\"updated_at\",\"merged_into_id\",\"work_email\",\"work_phone\" FROM hr_people) p ON p.user_id=a.user_id AND p.company_id=a.company_id JOIN (SELECT \"id\",\"person_id\",\"company_id\",\"branch_id\",\"department_id\",\"worker_type\",\"employment_type\",\"status\",\"start_date\",\"end_date\",\"manager_person_id\",\"job_title\",\"currency\",\"version\",\"created_by\",\"created_at\",\"updated_at\",\"pay_group_id\",\"level_id\",\"position_id\",\"self_service_top_level\" FROM hr_engagements) e ON e.person_id=p.id AND e.company_id=p.company_id WHERE a.company_id=t.company_id AND a.user_id=m.user_id AND iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(a.user_id,a.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(a.role,a.company_id,'support.work') AND iam_role_allows(a.role,a.company_id,'support.read') AND (a.scope_kind='holding' OR (a.scope_kind='branch' AND a.scope_node_id=t.branch_id) OR (a.scope_kind='department' AND a.scope_node_id=t.department_id)) AND p.status='active' AND p.merged_into_id IS NULL AND e.status='active' AND e.start_date<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') AND (e.end_date IS NULL OR e.end_date>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD')))) reviewer_count FROM support_tickets t WHERE t.company_id=? AND (?::text IS NULL OR t.branch_id=?) AND (?::text IS NULL OR t.department_id=?) AND EXISTS(SELECT 1 FROM (SELECT \"id\",\"user_id\",\"role\",\"modules_json\",\"company_id\",\"branch_id\",\"valid_from\",\"valid_until\",\"revoked_at\",\"version\",\"scope_kind\",\"scope_node_id\",\"parent_id\",\"source\",\"source_id\" FROM assignments) g WHERE g.id=? AND g.user_id=? AND g.role=? AND g.company_id=t.company_id\n   AND iam_assignment_active(g.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(g.user_id,g.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)\n   AND (g.scope_kind='holding' OR g.scope_kind='branch' AND g.scope_node_id=t.branch_id OR g.scope_kind='department' AND g.scope_node_id=t.department_id)\n   AND (iam_role_allows(g.role,g.company_id,'support.read') OR t.source_kind<>'sales_order' AND t.created_by=g.user_id AND iam_role_allows(g.role,g.company_id,'records.read'))) AND t.source_kind<>'field_service' ORDER BY t.updated_at DESC";

const actor = {
    companyId: 'company', branchId: 'branch', departmentId: 'department',
    assignmentId: 'assignment', userId: 'user', role: 'support.read',
} as unknown as Parameters<typeof supportTicketReadPredicate>[0];
const countSelect = (space: boolean): number => {
    let total = 0;
    const walk = (dir: string) => {
        for (const entry of readdirSync(dir, { withFileTypes: true })) {
            const path = resolve(dir, entry.name);
            if (entry.isDirectory()) walk(path);
            else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(space ? /SELECT /g : /SELECT/g) ?? []).length;
        }
    };
    walk(srcRoot);
    return total;
};

describe('Support read-tail r41 ticket work native kind', () => {
    test('the retired supportWork literal is inlined byte-for-byte in Rust', () => {
        expect(rustLiteral('TICKET_WORK')).toBe(goldenWork);
        expect(goldenWork.startsWith('SELECT t.*,CASE WHEN')).toBe(true);
        expect(goldenWork).toContain(' FROM support_tickets t WHERE t.company_id=? AND ');
        expect(goldenWork).toContain(" AND t.source_kind<>'field_service' ORDER BY t.updated_at DESC");
        expect(goldenWork).not.toContain(';');
    });

    test('supportAssignmentAccessSql embedded newline/pad whitespace is preserved exactly', () => {
        expect(goldenWork).toContain('g.company_id=t.company_id\n   AND iam_assignment_active(');
        expect(goldenWork).toContain('\n   AND (g.scope_kind=');
        expect(goldenWork).toContain("\n   AND (iam_role_allows(g.role,g.company_id,'support.read')");
        expect(goldenWork).toContain('owner_eligible');
        expect(goldenWork).toContain('reviewer_count');
    });

    test('the kind has the exact fixed arity and bind order', () => {
        expect(bridge).toContain("SUPPORT_RD_TICKET_WORK = 'support_rd_ticket_work'");
        expect(bridge).toContain('[SUPPORT_RD_TICKET_WORK]: 8');
        expect(registry).toContain('pub const RD_TICKET_WORK: &str = "support_rd_ticket_work";');
        expect(registry).toContain('    READ_TICKET_WORK,\n');
        expect(registry).toContain('(RD_TICKET_WORK, SQL_TICKET_WORK, 8usize)');
        expect((goldenWork.match(/\?/g) ?? []).length).toBe(8);
        expect([...args(actor), ...supportTicketReadPredicate(actor).bindings]).toEqual([
            'company', 'branch', 'branch', 'department', 'department',
            'assignment', 'user', 'support.read',
        ]);
        expect(workSource).toContain('nativeSupportReadAll<Row>(SUPPORT_RD_TICKET_WORK, [...args(a), ...supportTicketReadPredicate(a).bindings], write)');
        expect(workSource).not.toContain('${ticketEligibilitySelect(eligible, reviewers)}');
        expect(workSource).not.toContain('write.prepare(');
    });

    test('public export signature, purchase gate, locks and incident reader stay untouched', () => {
        expect(workSource).toContain('export async function supportWork(a: WorkContext)');
        expect(workSource).toContain('await runtime.purchase("support.tickets")');
        expect(workSource).toContain('if (!access.history) return []');
        expect(workSource).toContain('createSupportWork(');
        expect(workSource).toContain('sourceVisible');
        expect(src('public/support.ts')).toContain('supportWork');
        expect(src('adapters/support-incidents.ts')).toContain('nativeSupportReadAll<Row>(SUPPORT_RD_INCIDENT_WORK, [a.userId, ...args(a), ...permission.bindings], write)');
        expect(src('adapters/support-ticket-read.ts')).toContain('SELECT id FROM support_tickets WHERE id=? AND company_id=? FOR SHARE');
        expect(src('adapters/ticket-portal-unit.ts')).toContain('LOCK TABLE command_receipts,command_assertions,audit_events,outbox_events IN SHARE ROW EXCLUSIVE MODE');
        expect(registry).not.toContain('FOR SHARE');
        expect(registry).not.toContain('LOCK TABLE');
    });

    test('retirement:ts-statements drops 35->31 and the registry grows 49->50', () => {
        // The ticket-work caller rendered its SQL from shared helpers and had no
        // counted SELECT literal. r42 switched the incident work list to its own
        // native kind and retired the shared `eligibilitySelect` token, so the
        // owner SELECT count is now 31.
        expect(countSelect(true)).toBe(28);
        expect(countSelect(false)).toBe(28);
        expect(registry).toContain('assert_eq!(STATEMENTS.len(), 52);');
    });
});
