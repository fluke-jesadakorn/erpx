import { readFileSync, readdirSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { resolve } from 'node:path';
import { describe, expect, test, vi } from 'vitest';

vi.mock('server-only', () => ({}));

import {
  portalValidBindingRead,
  portalValidBindingSelect,
  supportTicketPortalValidBindings,
  supportTicketPortalCoreImage,
  supportTicketPortalImage,
} from '@erpx/server-support/adapters/ticket-portal-sources';

const srcRoot = resolve(__dirname, '../../../packages/server-support/src');
const src = (f: string) => readFileSync(resolve(srcRoot, f), 'utf8');
const head = (f: string) => execFileSync('git', ['show', `HEAD:packages/server-support/src/adapters/${f}`], { encoding: 'utf8' });

/** Exact pre-r29 staff single-binding outer read from HEAD. */
const staffBefore = 'SELECT * FROM (${supportTicketPortalValidBindings}) binding WHERE binding.id=?';
/** Exact pre-r19 shared outer prefix, retained as the r19 constant. */
const prefixBefore = `SELECT binding.* FROM (${supportTicketPortalValidBindings}) binding`;
/** Exact r20-rendered full/core portal images (unchanged by r29). */
const beforeFull = `WITH selected AS(SELECT * FROM support_ticket_portal_bindings WHERE id IN (?,?)) SELECT jsonb_build_array((SELECT jsonb_agg(to_jsonb(binding) ORDER BY binding.id) FROM selected binding),(SELECT jsonb_agg(to_jsonb(ticket) ORDER BY ticket.id) FROM support_tickets ticket WHERE EXISTS(SELECT 1 FROM selected binding WHERE binding.ticket_id=ticket.id)),(SELECT jsonb_agg(to_jsonb(event) ORDER BY event.id) FROM support_ticket_portal_events event WHERE EXISTS(SELECT 1 FROM selected binding WHERE binding.id=event.binding_id)),(SELECT jsonb_agg(to_jsonb(identity) ORDER BY identity.binding_id) FROM support_ticket_portal_identities identity WHERE EXISTS(SELECT 1 FROM selected binding WHERE binding.id=identity.binding_id)),(SELECT jsonb_agg(to_jsonb(read) ORDER BY read.id) FROM support_ticket_portal_reads read WHERE EXISTS(SELECT 1 FROM selected binding WHERE binding.id=read.binding_id)),(SELECT jsonb_agg(to_jsonb(response) ORDER BY response.id) FROM support_ticket_portal_responses response WHERE EXISTS(SELECT 1 FROM selected binding WHERE binding.id=response.binding_id)),(SELECT jsonb_agg(to_jsonb(ack) ORDER BY ack.id) FROM support_ticket_portal_acknowledgements ack WHERE EXISTS(SELECT 1 FROM selected binding WHERE binding.id=ack.binding_id)))::text image`;
const beforeCore = `WITH selected AS(SELECT * FROM support_ticket_portal_bindings WHERE id IN (?,?)) SELECT jsonb_build_array((SELECT jsonb_agg(to_jsonb(binding) ORDER BY binding.id) FROM selected binding),(SELECT jsonb_agg(to_jsonb(ticket) ORDER BY ticket.id) FROM support_tickets ticket WHERE EXISTS(SELECT 1 FROM selected binding WHERE binding.ticket_id=ticket.id)),(SELECT jsonb_agg(to_jsonb(event) ORDER BY event.id) FROM support_ticket_portal_events event WHERE EXISTS(SELECT 1 FROM selected binding WHERE binding.id=event.binding_id)),(SELECT jsonb_agg(to_jsonb(response) ORDER BY response.id) FROM support_ticket_portal_responses response WHERE EXISTS(SELECT 1 FROM selected binding WHERE binding.id=response.binding_id)))::text image`;

function capture() {
    return {
        prepare(sql: string) {
            return { bind: () => ({ first: async () => sql }) };
        },
    } as never;
}

describe('Support read-tail r29 valid-binding + image family', () => {
    test('staff single-binding outer read renders byte-identically to the HEAD literal', () => {
        expect(head('ticket-portal.ts')).toContain(staffBefore);
        const rendered = `${portalValidBindingRead('*')} WHERE binding.id=?`;
        expect(rendered).toBe(staffBefore.replace('${supportTicketPortalValidBindings}', supportTicketPortalValidBindings));
    });

    test('r19 prefix helper is retained and still renders byte-identically', () => {
        expect(portalValidBindingSelect).toBe(prefixBefore);
        expect(portalValidBindingSelect).toBe(portalValidBindingRead('binding.*'));
        expect(src('adapters/ticket-portal-sources.ts')).toContain('export const portalValidBindingSelect=');
    });

    test('one shared parameterized family, no inlined outer select at the staff caller', () => {
        expect(src('adapters/ticket-portal-sources.ts').match(/export const portalValidBindingRead=/g) ?? []).toHaveLength(1);
        expect(src('adapters/ticket-portal.ts')).toContain("portalValidBindingRead('*')");
        expect(src('adapters/ticket-portal.ts')).not.toContain(staffBefore);
        expect(src('adapters/ticket-portal-customer.ts')).toContain('portalValidBindingSelect');
        expect(src('adapters/ticket-portal.ts')).toContain('portalValidBindingSelect');
    });

    test('image family still renders byte-identically', async () => {
        expect(await supportTicketPortalImage(capture(), ['b1', 'b2'])).toBe(beforeFull);
        expect(await supportTicketPortalCoreImage(capture(), ['b1', 'b2'])).toBe(beforeCore);
    });

    test('locks, purchase gates and prior helpers stay inline', () => {
        const unit = src('adapters/ticket-portal-unit.ts');
        expect(unit).toContain('LOCK TABLE command_receipts,command_assertions,audit_events,outbox_events IN SHARE ROW EXCLUSIVE MODE');
        expect(unit).toContain('pg_advisory_xact_lock');
        expect(unit).toContain('SELECT ${scheduledFeaturePredicate(');
        const sources = src('adapters/ticket-portal-sources.ts');
        expect(sources).toContain('export const portalReplayProofSql=');
        expect(sources).toContain('export function supportTicketPortalSourceLocks');
        expect(sources).toContain('portalImageAggregateSql(');
    });

    test('retirement:ts-statements stays strictly below the r29 BEFORE=51 baseline', () => {
        let total = 0;
        const walk = (dir: string) => {
            for (const entry of readdirSync(dir, { withFileTypes: true })) {
                const path = resolve(dir, entry.name);
                if (entry.isDirectory()) walk(path);
                else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT/g) ?? []).length;
            }
        };
        walk(srcRoot);
        expect(total).toBe(50);
        expect(total).toBeLessThan(51);
    });
});
