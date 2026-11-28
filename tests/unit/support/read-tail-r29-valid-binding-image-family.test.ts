import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { beforeAll, describe, expect, test, vi } from 'vitest';

const { fragments } = vi.hoisted(() => ({ fragments: { valid: '', full: '', core: '' } }));

vi.mock('server-only', () => ({}));
vi.mock('@erpx/support-server/adapters/native-support-read', async (importOriginal) => {
    const original = await importOriginal<typeof import('../../../modules/support/server/src/adapters/native-support-read')>();
    return {
        ...original,
        nativeSupportReadFragment: async (kind: string) =>
            kind.endsWith('_valid_bindings') ? fragments.valid : kind.endsWith('_full') ? fragments.full : fragments.core,
    };
});

import {
    portalValidBindingRead,
    supportTicketPortalCoreImage,
    supportTicketPortalImage,
} from '@erpx/support-server/adapters/ticket-portal-sources';
import { packageClockSql, purchasedCapabilityPredicate } from '@erpx/commercial-server/public/purchased-capability-sql';

const srcRoot = resolve(__dirname, '../../../modules/support/server/src');
const src = (f: string) => readFileSync(resolve(srcRoot, f), 'utf8');
const registry = src('adapters/native_support_read_sql.rs');

const identitySql = (alias: string) =>
    `jsonb_build_object('companyId',${alias}.company_id,'branchId',${alias}.branch_id,'departmentId',${alias}.department_id,'sourceKind',${alias}.source_kind,'orderId',${alias}.order_id,'customerId',${alias}.customer_id,'sourceRecord',${alias}.source_record_json)`;
/** Exact pre-r29 valid-binding subquery from HEAD. */
const validBefore = `SELECT binding.* FROM support_ticket_portal_bindings binding JOIN support_tickets ticket ON ticket.id=binding.ticket_id AND ticket.company_id=binding.company_id AND ticket.source_kind<>'field_service' AND binding.source_identity=${identitySql('ticket')} JOIN command_receipts birth ON birth.command_id=binding.prepare_command_id AND birth.operation='support.ticket-portal.prepare' AND birth.record_id=binding.id AND birth.actor_id=binding.prepared_by AND birth.authority_json::jsonb->>'companyId'=binding.company_id AND birth.result_json::jsonb->>'id'=binding.id AND birth.result_json::jsonb->>'status'='draft' AND birth.result_json::jsonb->>'version'='1'`;
/** Exact pre-r19/r20 unambiguous outer read + image oracles from HEAD. */
const staffBefore = `SELECT * FROM (${validBefore}) binding WHERE binding.id=?`;
const prefixBefore = `SELECT binding.* FROM (${validBefore}) binding`;
const beforeFull = `WITH selected AS(SELECT * FROM support_ticket_portal_bindings WHERE id IN (?,?)) SELECT jsonb_build_array((SELECT jsonb_agg(to_jsonb(binding) ORDER BY binding.id) FROM selected binding),(SELECT jsonb_agg(to_jsonb(ticket) ORDER BY ticket.id) FROM support_tickets ticket WHERE EXISTS(SELECT 1 FROM selected binding WHERE binding.ticket_id=ticket.id)),(SELECT jsonb_agg(to_jsonb(event) ORDER BY event.id) FROM support_ticket_portal_events event WHERE EXISTS(SELECT 1 FROM selected binding WHERE binding.id=event.binding_id)),(SELECT jsonb_agg(to_jsonb(identity) ORDER BY identity.binding_id) FROM support_ticket_portal_identities identity WHERE EXISTS(SELECT 1 FROM selected binding WHERE binding.id=identity.binding_id)),(SELECT jsonb_agg(to_jsonb(read) ORDER BY read.id) FROM support_ticket_portal_reads read WHERE EXISTS(SELECT 1 FROM selected binding WHERE binding.id=read.binding_id)),(SELECT jsonb_agg(to_jsonb(response) ORDER BY response.id) FROM support_ticket_portal_responses response WHERE EXISTS(SELECT 1 FROM selected binding WHERE binding.id=response.binding_id)),(SELECT jsonb_agg(to_jsonb(ack) ORDER BY ack.id) FROM support_ticket_portal_acknowledgements ack WHERE EXISTS(SELECT 1 FROM selected binding WHERE binding.id=ack.binding_id)))::text image`;
const beforeCore = `WITH selected AS(SELECT * FROM support_ticket_portal_bindings WHERE id IN (?,?)) SELECT jsonb_build_array((SELECT jsonb_agg(to_jsonb(binding) ORDER BY binding.id) FROM selected binding),(SELECT jsonb_agg(to_jsonb(ticket) ORDER BY ticket.id) FROM support_tickets ticket WHERE EXISTS(SELECT 1 FROM selected binding WHERE binding.ticket_id=ticket.id)),(SELECT jsonb_agg(to_jsonb(event) ORDER BY event.id) FROM support_ticket_portal_events event WHERE EXISTS(SELECT 1 FROM selected binding WHERE binding.id=event.binding_id)),(SELECT jsonb_agg(to_jsonb(response) ORDER BY response.id) FROM support_ticket_portal_responses response WHERE EXISTS(SELECT 1 FROM selected binding WHERE binding.id=response.binding_id)))::text image`;
const purchased = ['support.tickets', 'support.portal']
    .map(capability => purchasedCapabilityPredicate('binding.company_id', capability, 'history', 'support_portal_clock.now_ms'))
    .join(' AND ');
const purchaseBefore = `EXISTS(WITH support_portal_clock AS MATERIALIZED(SELECT ${packageClockSql} now_ms) SELECT 1 FROM support_portal_clock WHERE ${purchased})`;

const aggregate = (sql: string) => sql.slice(sql.indexOf(') SELECT jsonb_build_array') + 2);

function rustFragment(name: string): string {
    const match = registry.match(new RegExp(`const SQL_${name}: &str =\\s*("(?:[^"\\\\]|\\\\.)*");`));
    if (!match) throw new Error(`missing SQL_${name}`);
    return JSON.parse(match[1]!) as string;
}

beforeAll(() => {
    fragments.valid = rustFragment('TICKET_PORTAL_VALID_BINDINGS');
    fragments.full = aggregate(beforeFull);
    fragments.core = aggregate(beforeCore);
});

function capture() {
    return {
        prepare(sql: string) {
            return { bind: () => ({ first: async () => sql }) };
        },
    } as never;
}

describe('Support read-tail r29 valid-binding + image family', () => {
    test('plans the native valid-binding fragment and pins the 1/4/9 tail shapes', () => {
        expect(fragments.valid).toBe(validBefore);
        expect(registry).toContain('pub const RD_TICKET_PORTAL_VALID_BINDINGS: &str = "support_rd_ticket_portal_valid_bindings";');
        expect(registry).toContain('tail_bind_counts: &[1, 4, 9]');
        expect(registry).toContain('const FRAGMENTS: &[NativeSupportReadFragment] = &[');
    });

    test('the static customer purchase scope is byte-identical in the native fragment', () => {
        expect(rustFragment('TICKET_PORTAL_CUSTOMER_PURCHASE')).toBe(purchaseBefore);
        expect(registry).toContain('pub const RD_TICKET_PORTAL_CUSTOMER_PURCHASE: &str = "support_rd_ticket_portal_customer_purchase";');
        const customer = src('adapters/ticket-portal-customer.ts');
        expect(customer).toContain('nativeSupportReadFragment(SUPPORT_RD_TICKET_PORTAL_CUSTOMER_PURCHASE,0)');
        expect(customer).toContain("scheduledFeaturePredicate('binding.company_id','support')");
        expect(customer).not.toContain('purchasedCapabilityPredicate');
        expect(customer).not.toContain('packageClockSql');
    });

    test('the staff 1-bind tail still renders byte-identically over the native head', async () => {
        expect(`${await portalValidBindingRead('*', 1)} WHERE binding.id=?`).toBe(staffBefore);
        expect(await portalValidBindingRead('binding.*', 9)).toBe(prefixBefore);
    });

    test('the caller owns only the closed 4/9-bind tails', () => {
        const staff = src('adapters/ticket-portal.ts');
        const customer = src('adapters/ticket-portal-customer.ts');
        expect(staff).toContain("portalValidBindingRead('*',1)");
        expect(staff).toContain("portalValidBindingRead('binding.*',9)");
        expect(customer).toContain("portalValidBindingRead('binding.*',4)");
        expect(staff).not.toContain('portalValidBindingSelect');
        expect(customer).not.toContain('portalValidBindingSelect');
        expect(src('adapters/ticket-portal-sources.ts')).not.toContain('supportTicketPortalValidBindings');
    });

    test('image family still renders byte-identically from the native aggregates', async () => {
        expect(await supportTicketPortalImage(capture(), ['b1', 'b2'])).toBe(beforeFull);
        expect(await supportTicketPortalCoreImage(capture(), ['b1', 'b2'])).toBe(beforeCore);
    });

    test('locks, purchase gates and prior helpers stay inline', () => {
        const unit = src('adapters/ticket-portal-unit.ts');
        expect(unit).toContain('LOCK TABLE command_receipts,command_assertions,audit_events,outbox_events IN SHARE ROW EXCLUSIVE MODE');
        expect(unit).toContain('pg_advisory_xact_lock');
        expect(unit).toContain('SELECT ${scheduledFeaturePredicate(');
        const sources = src('adapters/ticket-portal-sources.ts');
        expect(src('adapters/ticket-portal.ts')).toContain('SUPPORT_RD_TICKET_PORTAL_DECISION_REPLAY');
        expect(sources).toContain('export function supportTicketPortalSourceLocks');
        expect(sources).toContain('nativeSupportReadFragment(SUPPORT_RD_TICKET_PORTAL_IMAGE_AGGREGATE_FULL,0)');
    });

    test('retirement:ts-statements drops from the r11 AFTER=24 to 18', () => {
        let total = 0;
        const walk = (dir: string) => {
            for (const entry of readdirSync(dir, { withFileTypes: true })) {
                const path = resolve(dir, entry.name);
                if (entry.isDirectory()) walk(path);
                else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
            }
        };
        walk(srcRoot);
        expect(total).toBe(18);
        expect(total).toBeLessThan(24);
    });
});
