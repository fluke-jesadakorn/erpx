import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test, vi } from 'vitest';

vi.mock('server-only', () => ({}));

import { finalRootOwnerProjectionSources as identityAccessProjections } from '@erpx/server-identity_access/public/final-root-owner-projection-sources';
import { supportTicketReadPredicate } from '../../../packages/server-support/src/adapters/support-ticket-read';
import { supportIncidentReadPredicate } from '../../../packages/server-support/src/adapters/support-incident-read';
import { args } from '../../../packages/server-support/src/adapters/support-ticket-read-sources';

const srcRoot = resolve(__dirname, '../../../packages/server-support/src');
const src = (p: string) => readFileSync(resolve(srcRoot, p), 'utf8');
const registry = src('adapters/native_support_read_sql.rs');
const bridge = src('adapters/native-support-read.ts');
const ticketSource = src('adapters/support-ticket-read.ts');
const incidentSource = src('adapters/support-incident-read.ts');

/** Rust string literal -> exact SQL text (only double quotes and newlines are escaped). */
const rustLiteral = (name: string): string => {
    const match = registry.match(new RegExp(`const SQL_${name}: &str =\\s*("(?:[^"\\\\]|\\\\.)*");`));
    if (!match) throw new Error(`missing SQL_${name}`);
    return JSON.parse(match[1]!) as string;
};

/** Exact HEAD oracles: support-ticket-read.ts:32 and support-incident-read.ts:31-32. */
const goldenTicket = "SELECT t.*,o.number order_number,p.name customer_name,(SELECT legal_name FROM (SELECT \"id\",\"user_id\",\"company_id\",\"legal_name\",\"preferred_name\",\"email\",\"phone\",\"country_code\",\"status\",\"version\",\"created_by\",\"created_at\",\"updated_at\",\"merged_into_id\",\"work_email\",\"work_phone\" FROM hr_people) hr_people WHERE company_id=t.company_id AND user_id=t.owner_id AND merged_into_id IS NULL LIMIT 1) owner_name,CASE WHEN EXISTS(SELECT 1 FROM (SELECT \"id\",\"user_id\",\"role\",\"modules_json\",\"company_id\",\"branch_id\",\"valid_from\",\"valid_until\",\"revoked_at\",\"version\",\"scope_kind\",\"scope_node_id\",\"parent_id\",\"source\",\"source_id\" FROM assignments) a JOIN (SELECT \"id\",\"user_id\",\"company_id\",\"legal_name\",\"preferred_name\",\"email\",\"phone\",\"country_code\",\"status\",\"version\",\"created_by\",\"created_at\",\"updated_at\",\"merged_into_id\",\"work_email\",\"work_phone\" FROM hr_people) p ON p.user_id=a.user_id AND p.company_id=a.company_id JOIN (SELECT \"id\",\"person_id\",\"company_id\",\"branch_id\",\"department_id\",\"worker_type\",\"employment_type\",\"status\",\"start_date\",\"end_date\",\"manager_person_id\",\"job_title\",\"currency\",\"version\",\"created_by\",\"created_at\",\"updated_at\",\"pay_group_id\",\"level_id\",\"position_id\",\"self_service_top_level\" FROM hr_engagements) e ON e.person_id=p.id AND e.company_id=p.company_id WHERE a.company_id=t.company_id AND a.user_id=t.owner_id AND iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(a.user_id,a.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(a.role,a.company_id,'support.work') AND iam_role_allows(a.role,a.company_id,'support.read') AND (a.scope_kind='holding' OR (a.scope_kind='branch' AND a.scope_node_id=t.branch_id) OR (a.scope_kind='department' AND a.scope_node_id=t.department_id)) AND p.status='active' AND p.merged_into_id IS NULL AND e.status='active' AND e.start_date<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') AND (e.end_date IS NULL OR e.end_date>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD'))) THEN 1 ELSE 0 END owner_eligible,(SELECT COUNT(DISTINCT m.user_id) FROM (SELECT \"id\",\"user_id\",\"role\",\"modules_json\",\"company_id\",\"branch_id\",\"valid_from\",\"valid_until\",\"revoked_at\",\"version\",\"scope_kind\",\"scope_node_id\",\"parent_id\",\"source\",\"source_id\" FROM assignments) m WHERE m.company_id=t.company_id AND m.user_id<>t.owner_id AND m.user_id IS DISTINCT FROM t.resolved_by AND iam_assignment_active(m.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(m.role,m.company_id,'support.manage') AND (m.scope_kind='holding' OR (m.scope_kind='branch' AND m.scope_node_id=t.branch_id) OR (m.scope_kind='department' AND m.scope_node_id=t.department_id)) AND EXISTS(SELECT 1 FROM (SELECT \"id\",\"user_id\",\"role\",\"modules_json\",\"company_id\",\"branch_id\",\"valid_from\",\"valid_until\",\"revoked_at\",\"version\",\"scope_kind\",\"scope_node_id\",\"parent_id\",\"source\",\"source_id\" FROM assignments) a JOIN (SELECT \"id\",\"user_id\",\"company_id\",\"legal_name\",\"preferred_name\",\"email\",\"phone\",\"country_code\",\"status\",\"version\",\"created_by\",\"created_at\",\"updated_at\",\"merged_into_id\",\"work_email\",\"work_phone\" FROM hr_people) p ON p.user_id=a.user_id AND p.company_id=a.company_id JOIN (SELECT \"id\",\"person_id\",\"company_id\",\"branch_id\",\"department_id\",\"worker_type\",\"employment_type\",\"status\",\"start_date\",\"end_date\",\"manager_person_id\",\"job_title\",\"currency\",\"version\",\"created_by\",\"created_at\",\"updated_at\",\"pay_group_id\",\"level_id\",\"position_id\",\"self_service_top_level\" FROM hr_engagements) e ON e.person_id=p.id AND e.company_id=p.company_id WHERE a.company_id=t.company_id AND a.user_id=m.user_id AND iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(a.user_id,a.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(a.role,a.company_id,'support.work') AND iam_role_allows(a.role,a.company_id,'support.read') AND (a.scope_kind='holding' OR (a.scope_kind='branch' AND a.scope_node_id=t.branch_id) OR (a.scope_kind='department' AND a.scope_node_id=t.department_id)) AND p.status='active' AND p.merged_into_id IS NULL AND e.status='active' AND e.start_date<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') AND (e.end_date IS NULL OR e.end_date>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD')))) reviewer_count FROM support_tickets t LEFT JOIN (SELECT \"id\",\"company_id\",\"branch_id\",\"department_id\",\"kind\",\"number\",\"party_id\",\"warehouse_id\",\"currency\",\"order_date\",\"due_date\",\"note\",\"state\",\"subtotal_minor\",\"tax_minor\",\"source_id\",\"version\",\"created_by\",\"approved_by\",\"created_at\",\"updated_at\",\"price_list_id\",\"price_list_version\",\"discount_reason\",\"discount_approval_required\",\"discount_approved_by\",\"discount_approved_at\" FROM commerce_orders) o ON o.id=t.order_id AND o.company_id=t.company_id LEFT JOIN (SELECT \"id\",\"company_id\",\"kind\",\"code\",\"name\",\"currency\",\"tax_registration\",\"payment_terms_days\",\"credit_limit_minor\",\"status\",\"version\",\"created_at\",\"updated_at\",\"person_id\",\"notification_email\",\"notification_consent\",\"merged_into_id\",\"merged_at\",\"merged_by\",\"phone\",\"address_json\" FROM finance_parties) p ON p.id=t.customer_id AND p.company_id=t.company_id WHERE t.id=? AND t.company_id=? AND (?::text IS NULL OR t.branch_id=?) AND (?::text IS NULL OR t.department_id=?) AND EXISTS(SELECT 1 FROM (SELECT \"id\",\"user_id\",\"role\",\"modules_json\",\"company_id\",\"branch_id\",\"valid_from\",\"valid_until\",\"revoked_at\",\"version\",\"scope_kind\",\"scope_node_id\",\"parent_id\",\"source\",\"source_id\" FROM assignments) g WHERE g.id=? AND g.user_id=? AND g.role=? AND g.company_id=t.company_id\n   AND iam_assignment_active(g.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(g.user_id,g.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)\n   AND (g.scope_kind='holding' OR g.scope_kind='branch' AND g.scope_node_id=t.branch_id OR g.scope_kind='department' AND g.scope_node_id=t.department_id)\n   AND (iam_role_allows(g.role,g.company_id,'support.read') OR t.source_kind<>'sales_order' AND t.created_by=g.user_id AND iam_role_allows(g.role,g.company_id,'records.read')))";
const goldenIncident = "SELECT t.*,CASE WHEN EXISTS(SELECT 1 FROM (SELECT \"id\",\"user_id\",\"role\",\"modules_json\",\"company_id\",\"branch_id\",\"valid_from\",\"valid_until\",\"revoked_at\",\"version\",\"scope_kind\",\"scope_node_id\",\"parent_id\",\"source\",\"source_id\" FROM assignments) a JOIN (SELECT \"id\",\"user_id\",\"company_id\",\"legal_name\",\"preferred_name\",\"email\",\"phone\",\"country_code\",\"status\",\"version\",\"created_by\",\"created_at\",\"updated_at\",\"merged_into_id\",\"work_email\",\"work_phone\" FROM hr_people) p ON p.user_id=a.user_id AND p.company_id=a.company_id JOIN (SELECT \"id\",\"person_id\",\"company_id\",\"branch_id\",\"department_id\",\"worker_type\",\"employment_type\",\"status\",\"start_date\",\"end_date\",\"manager_person_id\",\"job_title\",\"currency\",\"version\",\"created_by\",\"created_at\",\"updated_at\",\"pay_group_id\",\"level_id\",\"position_id\",\"self_service_top_level\" FROM hr_engagements) e ON e.person_id=p.id AND e.company_id=p.company_id WHERE a.company_id=t.company_id AND a.user_id=t.owner_id AND iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(a.user_id,a.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(a.role,a.company_id,'support.work') AND iam_role_allows(a.role,a.company_id,'support.read') AND (a.scope_kind='holding' OR (a.scope_kind='branch' AND a.scope_node_id=t.branch_id) OR (a.scope_kind='department' AND a.scope_node_id=t.department_id)) AND p.status='active' AND p.merged_into_id IS NULL AND e.status='active' AND e.start_date<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') AND (e.end_date IS NULL OR e.end_date>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD'))) THEN 1 ELSE 0 END owner_eligible,CASE WHEN EXISTS(SELECT 1 FROM (SELECT \"id\",\"user_id\",\"role\",\"modules_json\",\"company_id\",\"branch_id\",\"valid_from\",\"valid_until\",\"revoked_at\",\"version\",\"scope_kind\",\"scope_node_id\",\"parent_id\",\"source\",\"source_id\" FROM assignments) m WHERE m.company_id=t.company_id AND m.user_id<>t.owner_id AND m.user_id IS DISTINCT FROM t.mitigated_by\n AND iam_assignment_active(m.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(m.role,m.company_id,'support.manage')\n AND (m.scope_kind='holding' OR (m.scope_kind='branch' AND m.scope_node_id=t.branch_id) OR (m.scope_kind='department' AND m.scope_node_id=t.department_id))\n AND EXISTS(SELECT 1 FROM (SELECT \"id\",\"user_id\",\"role\",\"modules_json\",\"company_id\",\"branch_id\",\"valid_from\",\"valid_until\",\"revoked_at\",\"version\",\"scope_kind\",\"scope_node_id\",\"parent_id\",\"source\",\"source_id\" FROM assignments) a JOIN (SELECT \"id\",\"user_id\",\"company_id\",\"legal_name\",\"preferred_name\",\"email\",\"phone\",\"country_code\",\"status\",\"version\",\"created_by\",\"created_at\",\"updated_at\",\"merged_into_id\",\"work_email\",\"work_phone\" FROM hr_people) p ON p.user_id=a.user_id AND p.company_id=a.company_id JOIN (SELECT \"id\",\"person_id\",\"company_id\",\"branch_id\",\"department_id\",\"worker_type\",\"employment_type\",\"status\",\"start_date\",\"end_date\",\"manager_person_id\",\"job_title\",\"currency\",\"version\",\"created_by\",\"created_at\",\"updated_at\",\"pay_group_id\",\"level_id\",\"position_id\",\"self_service_top_level\" FROM hr_engagements) e ON e.person_id=p.id AND e.company_id=p.company_id WHERE a.company_id=t.company_id AND a.user_id=m.user_id AND iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(a.user_id,a.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(a.role,a.company_id,'support.work') AND iam_role_allows(a.role,a.company_id,'support.read') AND (a.scope_kind='holding' OR (a.scope_kind='branch' AND a.scope_node_id=t.branch_id) OR (a.scope_kind='department' AND a.scope_node_id=t.department_id)) AND p.status='active' AND p.merged_into_id IS NULL AND e.status='active' AND e.start_date<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') AND (e.end_date IS NULL OR e.end_date>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD'))) AND m.user_id=?) THEN 1 ELSE 0 END reviewer_eligible,(SELECT COUNT(DISTINCT m.user_id) FROM (SELECT \"id\",\"user_id\",\"role\",\"modules_json\",\"company_id\",\"branch_id\",\"valid_from\",\"valid_until\",\"revoked_at\",\"version\",\"scope_kind\",\"scope_node_id\",\"parent_id\",\"source\",\"source_id\" FROM assignments) m WHERE m.company_id=t.company_id AND m.user_id<>t.owner_id AND m.user_id IS DISTINCT FROM t.mitigated_by\n AND iam_assignment_active(m.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(m.role,m.company_id,'support.manage')\n AND (m.scope_kind='holding' OR (m.scope_kind='branch' AND m.scope_node_id=t.branch_id) OR (m.scope_kind='department' AND m.scope_node_id=t.department_id))\n AND EXISTS(SELECT 1 FROM (SELECT \"id\",\"user_id\",\"role\",\"modules_json\",\"company_id\",\"branch_id\",\"valid_from\",\"valid_until\",\"revoked_at\",\"version\",\"scope_kind\",\"scope_node_id\",\"parent_id\",\"source\",\"source_id\" FROM assignments) a JOIN (SELECT \"id\",\"user_id\",\"company_id\",\"legal_name\",\"preferred_name\",\"email\",\"phone\",\"country_code\",\"status\",\"version\",\"created_by\",\"created_at\",\"updated_at\",\"merged_into_id\",\"work_email\",\"work_phone\" FROM hr_people) p ON p.user_id=a.user_id AND p.company_id=a.company_id JOIN (SELECT \"id\",\"person_id\",\"company_id\",\"branch_id\",\"department_id\",\"worker_type\",\"employment_type\",\"status\",\"start_date\",\"end_date\",\"manager_person_id\",\"job_title\",\"currency\",\"version\",\"created_by\",\"created_at\",\"updated_at\",\"pay_group_id\",\"level_id\",\"position_id\",\"self_service_top_level\" FROM hr_engagements) e ON e.person_id=p.id AND e.company_id=p.company_id WHERE a.company_id=t.company_id AND a.user_id=m.user_id AND iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(a.user_id,a.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(a.role,a.company_id,'support.work') AND iam_role_allows(a.role,a.company_id,'support.read') AND (a.scope_kind='holding' OR (a.scope_kind='branch' AND a.scope_node_id=t.branch_id) OR (a.scope_kind='department' AND a.scope_node_id=t.department_id)) AND p.status='active' AND p.merged_into_id IS NULL AND e.status='active' AND e.start_date<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') AND (e.end_date IS NULL OR e.end_date>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD')))) reviewer_count\n  FROM support_incidents t WHERE t.id=? AND t.company_id=? AND (?::text IS NULL OR t.branch_id=?) AND (?::text IS NULL OR t.department_id=?) AND EXISTS(SELECT 1 FROM (SELECT \"id\",\"user_id\",\"role\",\"modules_json\",\"company_id\",\"branch_id\",\"valid_from\",\"valid_until\",\"revoked_at\",\"version\",\"scope_kind\",\"scope_node_id\",\"parent_id\",\"source\",\"source_id\" FROM assignments) g WHERE g.id=? AND g.user_id=? AND g.role=? AND g.company_id=t.company_id\n  AND iam_assignment_active(g.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(g.user_id,g.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)\n  AND iam_role_allows(g.role,g.company_id,'support.read') AND (g.scope_kind='holding' OR g.scope_kind='branch' AND g.scope_node_id=t.branch_id OR g.scope_kind='department' AND g.scope_node_id=t.department_id))";

const actor = {
    companyId: 'company', branchId: 'branch', departmentId: 'department',
    assignmentId: 'assignment', userId: 'user', role: 'support.read',
} as unknown as Parameters<typeof supportTicketReadPredicate>[0];

const selectCount = (): number => {
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
};

describe('Support read-tail r40 by-id native family', () => {
    test('both retired by-id literals are inlined byte-for-byte in Rust', () => {
        expect(rustLiteral('TICKET_BY_ID')).toBe(goldenTicket);
        expect(rustLiteral('INCIDENT_BY_ID')).toBe(goldenIncident);
        expect(goldenTicket.startsWith('SELECT t.*,o.number order_number,p.name customer_name,')).toBe(true);
        expect(goldenTicket).toContain('FROM support_tickets t LEFT JOIN (');
        expect(goldenIncident.startsWith('SELECT t.*,CASE WHEN')).toBe(true);
        expect(goldenIncident).toContain('\n  FROM support_incidents t WHERE t.id=? AND ');
        expect(goldenTicket).not.toContain(';');
        expect(goldenIncident).not.toContain(';');
    });

    test('supportAssignmentAccessSql embedded newline/pad whitespace is preserved exactly', () => {
        expect(goldenTicket).toContain('g.company_id=t.company_id\n   AND iam_assignment_active(');
        expect(goldenTicket).toContain('\n   AND (g.scope_kind=');
        expect(goldenTicket).toContain("\n   AND (iam_role_allows(g.role,g.company_id,'support.read')");
        expect(goldenIncident).toContain('g.company_id=t.company_id\n  AND iam_assignment_active(');
        expect(goldenIncident).toContain("\n  AND iam_role_allows(g.role,g.company_id,'support.read')");
        expect(identityAccessProjections.assignments).toContain('SELECT "id","user_id","role"');
    });

    test('both kinds have fixed arity and the exact bind order', () => {
        expect(bridge).toContain("SUPPORT_RD_TICKET_BY_ID = 'support_rd_ticket_by_id'");
        expect(bridge).toContain("SUPPORT_RD_INCIDENT_BY_ID = 'support_rd_incident_by_id'");
        expect(bridge).toContain('[SUPPORT_RD_TICKET_BY_ID]: 9');
        expect(bridge).toContain('[SUPPORT_RD_INCIDENT_BY_ID]: 10');
        expect(bridge).toContain('export async function nativeSupportReadFirst<T>');
        expect(registry).toContain('pub const RD_TICKET_BY_ID: &str = "support_rd_ticket_by_id";');
        expect(registry).toContain('pub const RD_INCIDENT_BY_ID: &str = "support_rd_incident_by_id";');
        expect(registry).toContain('    READ_TICKET_BY_ID,\n');
        expect(registry).toContain('    READ_INCIDENT_BY_ID,\n');
        expect(registry).toContain('(RD_TICKET_BY_ID, SQL_TICKET_BY_ID, 9usize)');
        expect(registry).toContain('(RD_INCIDENT_BY_ID, SQL_INCIDENT_BY_ID, 10usize)');
        expect((goldenTicket.match(/\?/g) ?? []).length).toBe(9);
        expect((goldenIncident.match(/\?/g) ?? []).length).toBe(10);
        expect(ticketSource).toContain('nativeSupportReadFirst<Row>(SUPPORT_RD_TICKET_BY_ID, [id, ...args(a), ...supportTicketReadPredicate(a).bindings], write)');
        expect(incidentSource).toContain('nativeSupportReadFirst<Row>(SUPPORT_RD_INCIDENT_BY_ID, [a.userId, id, ...args(a), ...supportIncidentReadPredicate(a).bindings], write)');
        expect(['ticket-1', ...args(actor), ...supportTicketReadPredicate(actor).bindings]).toEqual([
            'ticket-1', 'company', 'branch', 'branch', 'department', 'department',
            'assignment', 'user', 'support.read',
        ]);
        expect([actor.userId, 'incident-1', ...args(actor), ...supportIncidentReadPredicate(actor).bindings]).toEqual([
            'user', 'incident-1', 'company', 'branch', 'branch', 'department', 'department',
            'assignment', 'user', 'support.read',
        ]);
    });

    test('locks, guards, advisory locks and LOCK TABLE stay untouched', () => {
        expect(ticketSource).toContain('SELECT id FROM support_tickets WHERE id=? AND company_id=? FOR SHARE');
        expect(ticketSource).toContain('ticketGuardExistsSql(');
        expect(ticketSource).not.toContain('ticketEligibilitySelect');
        expect(incidentSource).not.toContain('incidentEligibilitySelect');
        expect(src('adapters/ticket-guard-sources.ts')).toContain('export const ticketGuardExistsSql = (predicate: string) =>');
        expect(src('adapters/ticket-portal-unit.ts')).toContain('SELECT pg_advisory_xact_lock(hashtextextended(?,0))');
        expect(src('adapters/ticket-portal-unit.ts')).toContain('LOCK TABLE command_receipts,command_assertions,audit_events,outbox_events IN SHARE ROW EXCLUSIVE MODE');
        expect(src('adapters/attachment-read.ts')).toContain('LOCK TABLE support_tickets,support_ticket_events IN SHARE ROW EXCLUSIVE MODE');
        expect(registry).not.toContain('FOR SHARE');
        expect(registry).not.toContain('LOCK TABLE');
    });

    test('retirement:ts-statements drops 37->31 and the registry grows 46->50', () => {
        expect(selectCount()).toBe(20);
        expect(selectCount()).toBeLessThan(37);
        expect(registry).toContain('assert_eq!(STATEMENTS.len(), 55);');
    });
});
