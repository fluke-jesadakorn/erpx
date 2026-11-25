import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { beforeAll, describe, expect, test, vi } from 'vitest';

const { fragments } = vi.hoisted(() => ({ fragments: { valid: '' } }));

vi.mock('server-only', () => ({}));
vi.mock('@erpx/server-support/adapters/native-support-read', async (importOriginal) => {
    const original = await importOriginal<typeof import('../../../packages/server-support/src/adapters/native-support-read')>();
    return {
        ...original,
        nativeSupportReadFragment: async () => fragments.valid,
    };
});

import { portalValidBindingRead } from '@erpx/server-support/adapters/ticket-portal-sources';

const srcRoot = resolve(__dirname, '../../../packages/server-support/src');
const src = (p: string) => readFileSync(resolve(srcRoot, p), 'utf8');
const sources = src('adapters/ticket-portal-sources.ts');
const staff = src('adapters/ticket-portal.ts');
const customer = src('adapters/ticket-portal-customer.ts');
const registry = src('adapters/native_support_read_sql.rs');

const identitySql = (alias: string) =>
    `jsonb_build_object('companyId',${alias}.company_id,'branchId',${alias}.branch_id,'departmentId',${alias}.department_id,'sourceKind',${alias}.source_kind,'orderId',${alias}.order_id,'customerId',${alias}.customer_id,'sourceRecord',${alias}.source_record_json)`;
/** Exact pre-dedupe valid-binding subquery from HEAD. */
const validBefore = `SELECT binding.* FROM support_ticket_portal_bindings binding JOIN support_tickets ticket ON ticket.id=binding.ticket_id AND ticket.company_id=binding.company_id AND ticket.source_kind<>'field_service' AND binding.source_identity=${identitySql('ticket')} JOIN command_receipts birth ON birth.command_id=binding.prepare_command_id AND birth.operation='support.ticket-portal.prepare' AND birth.record_id=binding.id AND birth.actor_id=binding.prepared_by AND birth.authority_json::jsonb->>'companyId'=binding.company_id AND birth.result_json::jsonb->>'id'=binding.id AND birth.result_json::jsonb->>'status'='draft' AND birth.result_json::jsonb->>'version'='1'`;

beforeAll(() => { fragments.valid = validBefore; });

describe('Support read-tail r19 valid-binding outer read family', () => {
    test('plans the byte-identical native valid-binding fragment', async () => {
        const head = await portalValidBindingRead('binding.*', 9);
        expect(head).toBe(`SELECT binding.* FROM (${validBefore}) binding`);
        expect(registry).toContain('pub const RD_TICKET_PORTAL_VALID_BINDINGS: &str = "support_rd_ticket_portal_valid_bindings";');
        expect(registry).toContain('tail_bind_counts: &[1, 4, 9]');
        expect(sources).not.toContain('supportTicketPortalValidBindings');
        expect(sources).not.toContain('portalValidBindingSelect');
    });

    test('each caller owns only its closed tail shape', () => {
        expect(sources.match(/export async function portalValidBindingRead\(/g) ?? []).toHaveLength(1);
        expect(staff).toContain("portalValidBindingRead('*',1)");
        expect(staff).toContain("portalValidBindingRead('binding.*',9)");
        expect(customer).toContain("portalValidBindingRead('binding.*',4)");
        expect(staff).not.toContain('SELECT binding.* FROM (');
        expect(customer).not.toContain('SELECT binding.* FROM (');
    });

    test('retirement:ts-statements drops from the r11 AFTER=24 to 20', () => {
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
        expect(total).toBeLessThan(24);
    });
});
