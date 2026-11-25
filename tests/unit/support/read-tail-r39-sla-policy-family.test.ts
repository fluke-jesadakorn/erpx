import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test, vi } from 'vitest';

vi.mock('server-only', () => ({}));

import { supportEligibleFlagSql, supportReviewerCountSql } from '../../../packages/server-support/src/adapters/eligibility-projection-sources';
import { supportRecipientEligibility } from '../../../packages/server-support/src/adapters/support-recipient';

const srcRoot = resolve(__dirname, '../../../packages/server-support/src');
const src = (p: string) => readFileSync(resolve(srcRoot, p), 'utf8');
const sla = src('adapters/support-sla.ts');
const bridge = src('adapters/native-support-read.ts');
const registry = src('adapters/native_support_read_sql.rs');

/** Exact projections inlined from the retired support-sla.ts:11-22 literals. */
const policyRecipient = supportRecipientEligibility()
    .replaceAll('t.company_id', 'p.company_id')
    .replaceAll('t.owner_id', 'm.user_id')
    .replaceAll('t.branch_id', 'NULL::text')
    .replaceAll('t.department_id', 'NULL::text');
const reviewerSql = `m.company_id=p.company_id AND m.user_id<>p.created_by AND m.scope_kind='holding' AND iam_assignment_active(m.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(m.role,m.company_id,'support.manage') AND ${policyRecipient}`;
const flagSql = supportEligibleFlagSql(reviewerSql, 'actor_eligible');
const beforeWorkspace = `SELECT p.*,${flagSql} FROM support_sla_policies p WHERE p.company_id=? ORDER BY p.created_at DESC,p.id`;
const beforeWork = `SELECT p.*,${flagSql},${supportReviewerCountSql(reviewerSql)} reviewers FROM support_sla_policies p WHERE p.company_id=? ORDER BY p.updated_at DESC`;

/** Rust string literal -> exact SQL text (the literals carry only " escapes). */
const rustLiteral = (name: string): string => {
    const match = registry.match(new RegExp(`const SQL_${name}: &str =\\s*("(?:[^"\\\\]|\\\\.)*");`));
    if (!match) throw new Error(`missing SQL_${name}`);
    return JSON.parse(match[1]!) as string;
};

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

describe('Support SLA policy workspace/work native read family (r39)', () => {
    test('both retired owner literals are inlined byte-for-byte in Rust', () => {
        expect(rustLiteral('SLA_POLICY_WORKSPACE')).toBe(beforeWorkspace);
        expect(rustLiteral('SLA_POLICY_WORK')).toBe(beforeWork);
        expect(registry).toContain('pub const RD_SLA_POLICY_WORKSPACE: &str = "support_rd_sla_policy_workspace";');
        expect(registry).toContain('pub const RD_SLA_POLICY_WORK: &str = "support_rd_sla_policy_work";');
        for (const sql of [beforeWorkspace, beforeWork]) {
            expect(sql.startsWith('SELECT p.*,')).toBe(true);
            expect(sql).toContain("iam_role_allows(m.role,m.company_id,'support.manage')");
            expect(sql).toContain("a.scope_node_id=NULL::text");
            expect(sql).toContain('actor_eligible');
            expect(sql).toContain('FROM support_sla_policies p WHERE p.company_id=?');
            expect(sql).not.toContain(';');
        }
        expect(beforeWork).toContain('reviewers FROM support_sla_policies p WHERE p.company_id=? ORDER BY p.updated_at DESC');
        expect(beforeWorkspace).toContain('ORDER BY p.created_at DESC,p.id');
    });

    test('both kinds have fixed arity 2 and cross [userId, companyId] in bind order', () => {
        expect(bridge).toContain("SUPPORT_RD_SLA_POLICY_WORKSPACE = 'support_rd_sla_policy_workspace'");
        expect(bridge).toContain("SUPPORT_RD_SLA_POLICY_WORK = 'support_rd_sla_policy_work'");
        expect(bridge).toContain('[SUPPORT_RD_SLA_POLICY_WORKSPACE]: 2');
        expect(bridge).toContain('[SUPPORT_RD_SLA_POLICY_WORK]: 2');
        expect(registry).toContain('    READ_SLA_POLICY_WORKSPACE,\n');
        expect(registry).toContain('    READ_SLA_POLICY_WORK,\n');
        expect(registry).toContain('(RD_SLA_POLICY_WORKSPACE, SQL_SLA_POLICY_WORKSPACE, 2usize)');
        expect(registry).toContain('(RD_SLA_POLICY_WORK, SQL_SLA_POLICY_WORK, 2usize)');
        for (const sql of [beforeWorkspace, beforeWork]) expect((sql.match(/\?/g) ?? []).length).toBe(2);
        expect(sla).toContain('nativeSupportReadAll<Row>(SUPPORT_RD_SLA_POLICY_WORKSPACE, [a.userId, a.companyId], write)');
        expect(sla).toContain('nativeSupportReadAll<Row>(SUPPORT_RD_SLA_POLICY_WORK, [a.userId, a.companyId], write)');
    });

    test('the SELECT-token helper is deleted from the owner source', () => {
        expect(sla).not.toContain('policyEligibilitySelect');
        expect(sla).not.toContain('supportEligibleFlagSql');
        expect(sla).not.toContain('supportReviewerCountSql');
        expect(sla).not.toContain('supportRecipientEligibility');
        expect(sla).not.toContain('SELECT p.*,');
        expect(sla).toContain('SUPPORT_RD_SLA_POLICY_POINT');
        expect(sla).toContain('SUPPORT_RD_SLA_WORKSPACE_TICKETS');
    });

    test('locks, advisory and LOCK TABLE stay untouched; eligibility helpers retired', () => {
        expect(src('adapters/support-ticket-read.ts')).toContain('SELECT id FROM support_tickets WHERE id=? AND company_id=? FOR SHARE');
        expect(src('adapters/support-ticket-read.ts')).not.toContain('ticketEligibilitySelect');
        expect(src('adapters/attachment-read.ts')).toContain('LOCK TABLE support_tickets,support_ticket_events IN SHARE ROW EXCLUSIVE MODE');
        expect(src('adapters/ticket-portal-unit.ts')).toContain('SELECT pg_advisory_xact_lock(hashtextextended(?,0))');
        expect(src('adapters/ticket-portal-unit.ts')).toContain('LOCK TABLE command_receipts,command_assertions,audit_events,outbox_events IN SHARE ROW EXCLUSIVE MODE');
        expect(src('adapters/support-incident-read.ts')).not.toContain('supportEligibleFlagSql(reviewerPredicate');
        expect(src('adapters/support-incident-read.ts')).not.toContain('incidentEligibilitySelect');
        expect(src('adapters/support-ticket-read-sources.ts')).toContain('supportReviewerCountSql(');
        expect(registry).not.toContain('LOCK TABLE support_sla_policies');
    });

    test('retirement:ts-statements 37->31 and the registry grows 46->50', () => {
        expect(selectCount()).toBe(31);
        expect(selectCount()).toBeLessThan(38);
        expect(registry).toContain('assert_eq!(STATEMENTS.len(), 50);');
    });
});
