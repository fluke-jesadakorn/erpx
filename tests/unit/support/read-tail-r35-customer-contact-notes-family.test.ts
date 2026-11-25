import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';

const srcRoot = resolve(__dirname, '../../../packages/server-support/src');
const src = (f: string) => readFileSync(resolve(srcRoot, f), 'utf8');
const support = src('adapters/support.ts');
const bridge = src('adapters/native-support-read.ts');
const registry = src('adapters/native_support_read_sql.rs');

const kind = 'support_rd_customer_contact_notes';
const retiredSelect = 'SELECT id,version,channel,occurred_at,body FROM (SELECT "id","company_id","customer_id","order_id","channel","occurred_at","body","status","version","created_by","created_at","updated_at" FROM customer_contact_notes) customer_contact_notes WHERE order_id=? AND company_id=? AND status=\'active\' ORDER BY occurred_at DESC';
const rustLiteral = retiredSelect.replaceAll('"', '\\"');

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

describe('Support read-tail r35 customer-contact-notes native kind', () => {
    test('the detail notes wrapper is a bridge call with two binds and no private SELECT', () => {
        expect(support).toContain('SUPPORT_RD_CUSTOMER_CONTACT_NOTES,SUPPORT_RD_SLA_POLICY_APPROVED');
        expect(support).toContain(
            'notes: async (a, orderId) => { const write = await runtime.database(); return (await nativeSupportReadAll<Row>(SUPPORT_RD_CUSTOMER_CONTACT_NOTES, [orderId, a.companyId], write)).results; },'
        );
        expect(support).not.toContain(retiredSelect);
        expect(support).not.toContain('salesProjectionSources');
        expect(support.match(/SUPPORT_RD_CUSTOMER_CONTACT_NOTES/g) ?? []).toHaveLength(2);
        expect(retiredSelect.match(/\?/g) ?? []).toHaveLength(2);
        expect(retiredSelect).toContain("order_id=? AND company_id=? AND status='active' ORDER BY occurred_at DESC");
    });

    test('the bridge exposes the kind with its fixed 2-bind arity', () => {
        expect(bridge).toContain(`SUPPORT_RD_CUSTOMER_CONTACT_NOTES = '${kind}'`);
        expect(bridge).toContain('[SUPPORT_RD_CUSTOMER_CONTACT_NOTES]: 2,');
        expect(bridge.match(/SUPPORT_RD_CUSTOMER_CONTACT_NOTES/g) ?? []).toHaveLength(2);
    });

    test('the Rust registry owns the retired literal verbatim at 36->37 kinds', () => {
        expect(registry).toContain(`RD_CUSTOMER_CONTACT_NOTES: &str = "${kind}"`);
        expect(registry).toContain(`"${rustLiteral}"`);
        expect(registry).toContain('kind: RD_CUSTOMER_CONTACT_NOTES,');
        expect(registry).toContain('    READ_CUSTOMER_CONTACT_NOTES,\n');
        expect(registry).toContain('assert_eq!(STATEMENTS.len(), 50);');
        expect(registry).toContain('RD_CUSTOMER_CONTACT_NOTES, SQL_CUSTOMER_CONTACT_NOTES, 2usize');
        expect(registry).toContain('customer_contact_notes_read_renders_the_retired_owner_literal_byte_for_byte');
        expect(registry).toContain('let notes = read_statement(RD_CUSTOMER_CONTACT_NOTES).expect("customer contact notes read");');
        expect(registry).toContain('sql.contains("customer_contact_notes")');
    });

    test('locks, advisory and other registered kinds stay untouched', () => {
        expect(src('adapters/ticket-portal-sources.ts')).toContain('LOCK TABLE support_tickets,support_ticket_events');
        expect(src('adapters/ticket-portal-unit.ts')).toContain('pg_advisory_xact_lock');
        expect(registry).not.toMatch(/LOCK TABLE|pg_advisory_xact_lock|workflow_command_assertion/);
        for (const kept of [
            'SUPPORT_RD_TICKET_RECIPIENTS',
            'SUPPORT_RD_SLA_ALERTS',
            'SUPPORT_RD_TICKET_PORTAL_TICKET_IMAGE',
            'SUPPORT_RD_SLA_POLICY_APPROVED',
        ]) expect(bridge).toContain(kept);
        for (const kept of [
            'READ_TICKET_RECIPIENTS',
            'READ_SLA_ALERTS',
            'READ_TICKET_PORTAL_TICKET_IMAGE',
            'READ_SLA_POLICY_APPROVED',
        ]) expect(registry).toContain(`    ${kept},\n`);
    });

    test('retirement:ts-statements is strictly below the r34 AFTER=44 baseline', () => {
        expect(selectCount()).toBe(32);
        expect(selectCount()).toBeLessThan(44);
    });
});
