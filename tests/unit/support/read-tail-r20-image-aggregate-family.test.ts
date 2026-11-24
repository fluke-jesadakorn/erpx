import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test, vi } from 'vitest';

vi.mock('server-only', () => ({}));

import { supportTicketPortalCoreImage, supportTicketPortalImage } from '@erpx/server-support/adapters/ticket-portal-sources';

const srcRoot = resolve(__dirname, '../../../packages/server-support/src');
const shared = readFileSync(resolve(srcRoot, 'adapters/ticket-portal-image-sql.ts'), 'utf8');
const sources = readFileSync(resolve(srcRoot, 'adapters/ticket-portal-sources.ts'), 'utf8');

/** Exact pre-dedupe (r20 BEFORE=59) renderings from HEAD. */
const beforeFull = `WITH selected AS(SELECT * FROM support_ticket_portal_bindings WHERE id IN (?,?)) SELECT jsonb_build_array((SELECT jsonb_agg(to_jsonb(binding) ORDER BY binding.id) FROM selected binding),(SELECT jsonb_agg(to_jsonb(ticket) ORDER BY ticket.id) FROM support_tickets ticket WHERE EXISTS(SELECT 1 FROM selected binding WHERE binding.ticket_id=ticket.id)),(SELECT jsonb_agg(to_jsonb(event) ORDER BY event.id) FROM support_ticket_portal_events event WHERE EXISTS(SELECT 1 FROM selected binding WHERE binding.id=event.binding_id)),(SELECT jsonb_agg(to_jsonb(identity) ORDER BY identity.binding_id) FROM support_ticket_portal_identities identity WHERE EXISTS(SELECT 1 FROM selected binding WHERE binding.id=identity.binding_id)),(SELECT jsonb_agg(to_jsonb(read) ORDER BY read.id) FROM support_ticket_portal_reads read WHERE EXISTS(SELECT 1 FROM selected binding WHERE binding.id=read.binding_id)),(SELECT jsonb_agg(to_jsonb(response) ORDER BY response.id) FROM support_ticket_portal_responses response WHERE EXISTS(SELECT 1 FROM selected binding WHERE binding.id=response.binding_id)),(SELECT jsonb_agg(to_jsonb(ack) ORDER BY ack.id) FROM support_ticket_portal_acknowledgements ack WHERE EXISTS(SELECT 1 FROM selected binding WHERE binding.id=ack.binding_id)))::text image`;

const beforeCore = `WITH selected AS(SELECT * FROM support_ticket_portal_bindings WHERE id IN (?,?)) SELECT jsonb_build_array((SELECT jsonb_agg(to_jsonb(binding) ORDER BY binding.id) FROM selected binding),(SELECT jsonb_agg(to_jsonb(ticket) ORDER BY ticket.id) FROM support_tickets ticket WHERE EXISTS(SELECT 1 FROM selected binding WHERE binding.ticket_id=ticket.id)),(SELECT jsonb_agg(to_jsonb(event) ORDER BY event.id) FROM support_ticket_portal_events event WHERE EXISTS(SELECT 1 FROM selected binding WHERE binding.id=event.binding_id)),(SELECT jsonb_agg(to_jsonb(response) ORDER BY response.id) FROM support_ticket_portal_responses response WHERE EXISTS(SELECT 1 FROM selected binding WHERE binding.id=response.binding_id)))::text image`;

/** Capture the exact SQL each reader prepares without executing it. */
function capture() {
    return {
        prepare(sql: string) {
            return { bind: () => ({ first: async () => sql }) };
        },
    } as never;
}

describe('Support read-tail r20 portal image aggregate family', () => {
    test('renders the full portal image byte-identically', async () => {
        expect(await supportTicketPortalImage(capture(), ['b1', 'b2'])).toBe(beforeFull);
    });

    test('renders the core portal image byte-identically', async () => {
        expect(await supportTicketPortalCoreImage(capture(), ['b1', 'b2'])).toBe(beforeCore);
    });

    test('one shared parameterized aggregate; no inlined outer image SQL at callers', () => {
        expect(shared.match(/export const portalImageAggregateSql\s*=/g) ?? []).toHaveLength(1);
        expect(shared).toContain('SELECT jsonb_build_array(${images.join(\',\')})::text image');
        expect(sources.match(/portalImageAggregateSql\(\[/g) ?? []).toHaveLength(2);
        expect(sources).not.toContain('SELECT jsonb_build_array(');
        expect(sources).not.toContain(')::text image`');
    });

    test('keeps the prior image-fragment family intact', () => {
        expect(sources.match(/portalImageSelectedCte\(bindingIds\)/g) ?? []).toHaveLength(2);
        expect(sources.match(/portalBindingImageSql\(\)/g) ?? []).toHaveLength(2);
        expect(sources.match(/portalBoundImageSql\(/g) ?? []).toHaveLength(9);
    });

    test('retirement:ts-statements stays strictly below the r20 BEFORE=59 baseline', () => {
        let total = 0;
        const walk = (dir: string) => {
            for (const entry of readdirSync(dir, { withFileTypes: true })) {
                const path = resolve(dir, entry.name);
                if (entry.isDirectory()) walk(path);
                else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT/g) ?? []).length;
            }
        };
        walk(srcRoot);
        expect(total).toBeLessThan(59);
    });
});
