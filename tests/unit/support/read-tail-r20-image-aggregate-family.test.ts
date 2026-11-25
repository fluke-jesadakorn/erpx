import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { beforeAll, describe, expect, test, vi } from 'vitest';

const { fragments } = vi.hoisted(() => ({ fragments: { full: '', core: '' } }));

vi.mock('server-only', () => ({}));
vi.mock('@erpx/server-support/adapters/native-support-read', async (importOriginal) => {
    const original = await importOriginal<typeof import('../../../packages/server-support/src/adapters/native-support-read')>();
    return {
        ...original,
        nativeSupportReadFragment: async (kind: string) =>
            kind.endsWith('_full') ? fragments.full : fragments.core,
    };
});

import { supportTicketPortalCoreImage, supportTicketPortalImage } from '@erpx/server-support/adapters/ticket-portal-sources';

const srcRoot = resolve(__dirname, '../../../packages/server-support/src');
const shared = readFileSync(resolve(srcRoot, 'adapters/ticket-portal-image-sql.ts'), 'utf8');
const sources = readFileSync(resolve(srcRoot, 'adapters/ticket-portal-sources.ts'), 'utf8');

/** Exact pre-dedupe (r20 BEFORE=59) renderings from HEAD. */
const beforeFull = `WITH selected AS(SELECT * FROM support_ticket_portal_bindings WHERE id IN (?,?)) SELECT jsonb_build_array((SELECT jsonb_agg(to_jsonb(binding) ORDER BY binding.id) FROM selected binding),(SELECT jsonb_agg(to_jsonb(ticket) ORDER BY ticket.id) FROM support_tickets ticket WHERE EXISTS(SELECT 1 FROM selected binding WHERE binding.ticket_id=ticket.id)),(SELECT jsonb_agg(to_jsonb(event) ORDER BY event.id) FROM support_ticket_portal_events event WHERE EXISTS(SELECT 1 FROM selected binding WHERE binding.id=event.binding_id)),(SELECT jsonb_agg(to_jsonb(identity) ORDER BY identity.binding_id) FROM support_ticket_portal_identities identity WHERE EXISTS(SELECT 1 FROM selected binding WHERE binding.id=identity.binding_id)),(SELECT jsonb_agg(to_jsonb(read) ORDER BY read.id) FROM support_ticket_portal_reads read WHERE EXISTS(SELECT 1 FROM selected binding WHERE binding.id=read.binding_id)),(SELECT jsonb_agg(to_jsonb(response) ORDER BY response.id) FROM support_ticket_portal_responses response WHERE EXISTS(SELECT 1 FROM selected binding WHERE binding.id=response.binding_id)),(SELECT jsonb_agg(to_jsonb(ack) ORDER BY ack.id) FROM support_ticket_portal_acknowledgements ack WHERE EXISTS(SELECT 1 FROM selected binding WHERE binding.id=ack.binding_id)))::text image`;

const beforeCore = `WITH selected AS(SELECT * FROM support_ticket_portal_bindings WHERE id IN (?,?)) SELECT jsonb_build_array((SELECT jsonb_agg(to_jsonb(binding) ORDER BY binding.id) FROM selected binding),(SELECT jsonb_agg(to_jsonb(ticket) ORDER BY ticket.id) FROM support_tickets ticket WHERE EXISTS(SELECT 1 FROM selected binding WHERE binding.ticket_id=ticket.id)),(SELECT jsonb_agg(to_jsonb(event) ORDER BY event.id) FROM support_ticket_portal_events event WHERE EXISTS(SELECT 1 FROM selected binding WHERE binding.id=event.binding_id)),(SELECT jsonb_agg(to_jsonb(response) ORDER BY response.id) FROM support_ticket_portal_responses response WHERE EXISTS(SELECT 1 FROM selected binding WHERE binding.id=response.binding_id)))::text image`;

const aggregate = (sql: string) => sql.slice(sql.indexOf(') SELECT jsonb_build_array') + 2);

beforeAll(() => {
    fragments.full = aggregate(beforeFull);
    fragments.core = aggregate(beforeCore);
});

/** Capture the exact SQL each reader prepares without executing it. */
function capture() {
    return {
        prepare(sql: string) {
            return { bind: () => ({ first: async () => sql }) };
        },
    } as never;
}

describe('Support read-tail r20 portal image aggregate family', () => {
    test('renders the full portal image byte-identically from the native aggregate', async () => {
        expect(await supportTicketPortalImage(capture(), ['b1', 'b2'])).toBe(beforeFull);
    });

    test('renders the core portal image byte-identically from the native aggregate', async () => {
        expect(await supportTicketPortalCoreImage(capture(), ['b1', 'b2'])).toBe(beforeCore);
    });

    test('the native aggregate fragments carry the retired image SQL and no binds', () => {
        expect(sources).toContain('nativeSupportReadFragment(SUPPORT_RD_TICKET_PORTAL_IMAGE_AGGREGATE_FULL,0)');
        expect(sources).toContain('nativeSupportReadFragment(SUPPORT_RD_TICKET_PORTAL_IMAGE_AGGREGATE_CORE,0)');
        expect(sources).not.toContain('SELECT jsonb_build_array(');
        expect(sources).not.toContain('portalBoundImageSql');
        expect(sources).not.toContain('portalImageAggregateSql');
        expect(shared).not.toContain('readTailR44ImageAggregateSql');
        expect(shared).not.toContain('portalBoundImageSql');
        expect(shared).toContain('export const portalImageSelectedCte =');
    });

    test('keeps the selected-bindings CTE as the only local image fragment', () => {
        expect(sources.match(/portalImageSelectedCte\(bindingIds\)/g) ?? []).toHaveLength(2);
        expect(shared.match(/readTailR43ColumnsFromSql\('\*'/g) ?? []).toHaveLength(1);
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
