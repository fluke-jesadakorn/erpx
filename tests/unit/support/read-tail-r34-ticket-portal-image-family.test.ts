import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';

const srcRoot = resolve(__dirname, '../../../packages/server-support/src');
const src = (f: string) => readFileSync(resolve(srcRoot, f), 'utf8');
const portalSources = src('adapters/ticket-portal-sources.ts');
const bridge = src('adapters/native-support-read.ts');
const registry = src('adapters/native_support_read_sql.rs');

const kind = 'support_rd_ticket_portal_ticket_image';
const retiredSelect = 'SELECT to_jsonb(ticket)::text image FROM support_tickets ticket WHERE ticket.id=?';

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

describe('Support read-tail r34 ticket-portal ticket-image native kind', () => {
    test('the TS wrapper is a bridge call with one bind and no private SELECT', () => {
        expect(portalSources).toContain("import {nativeSupportReadAll,SUPPORT_RD_TICKET_PORTAL_TICKET_IMAGE} from './native-support-read';");
        expect(portalSources).toContain(
            'export async function supportTicketPortalTicketImage(database:SqlDatabase,ticketId:string){return nativeSupportReadAll<{image:string}>(SUPPORT_RD_TICKET_PORTAL_TICKET_IMAGE,[ticketId],database).then(result => result.results[0]?.image ?? null);}'
        );
        expect(portalSources).not.toContain(retiredSelect);
        expect(portalSources.match(/supportTicketPortalTicketImage\(/g) ?? []).toHaveLength(1);
    });

    test('the bridge exposes the kind with its fixed 1-bind arity', () => {
        expect(bridge).toContain(`SUPPORT_RD_TICKET_PORTAL_TICKET_IMAGE = '${kind}'`);
        expect(bridge).toContain('[SUPPORT_RD_TICKET_PORTAL_TICKET_IMAGE]: 1,');
        expect(bridge.match(/SUPPORT_RD_TICKET_PORTAL_TICKET_IMAGE/g) ?? []).toHaveLength(2);
    });

    test('the Rust registry owns the retired literal verbatim at 35->36 kinds', () => {
        expect(registry).toContain(`RD_TICKET_PORTAL_TICKET_IMAGE: &str = "${kind}"`);
        expect(registry).toContain(`"${retiredSelect}"`);
        expect(registry).toContain('kind: RD_TICKET_PORTAL_TICKET_IMAGE,');
        expect(registry).toContain('    READ_TICKET_PORTAL_TICKET_IMAGE,\n');
        expect(registry).toContain('assert_eq!(STATEMENTS.len(), 36);');
        expect(registry).toContain('                RD_TICKET_PORTAL_TICKET_IMAGE,\n                SQL_TICKET_PORTAL_TICKET_IMAGE,\n                1usize,');
        expect(registry).toContain('ticket_portal_ticket_image_read_renders_the_retired_owner_literal_byte_for_byte');
    });

    test('locks, advisory and other registered kinds stay untouched', () => {
        expect(portalSources).toContain('LOCK TABLE support_tickets,support_ticket_events');
        expect(src('adapters/ticket-portal-unit.ts')).toContain('pg_advisory_xact_lock');
        expect(registry).not.toMatch(/LOCK TABLE|pg_advisory_xact_lock|workflow_command_assertion/);
        for (const kept of [
            'SUPPORT_RD_TICKET_PORTAL_RESPONSE',
            'SUPPORT_RD_TICKET_PORTAL_ACKNOWLEDGED_AT',
            'SUPPORT_RD_SLA_ALERTS',
            'SUPPORT_RD_TICKET_RECIPIENTS',
        ]) expect(bridge).toContain(kept);
        for (const kept of [
            'READ_TICKET_PORTAL_RESPONSE',
            'READ_TICKET_PORTAL_ACKNOWLEDGED_AT',
            'READ_SLA_ALERTS',
            'READ_TICKET_RECIPIENTS',
        ]) expect(registry).toContain(`    ${kept},\n`);
    });

    test('retirement:ts-statements is strictly below the r34 BEFORE=45 baseline', () => {
        expect(selectCount()).toBe(44);
        expect(selectCount()).toBeLessThan(45);
    });
});
