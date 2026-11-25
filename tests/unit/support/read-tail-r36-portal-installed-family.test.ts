import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';

const srcRoot = resolve(__dirname, '../../../packages/server-support/src');
const src = (f: string) => readFileSync(resolve(srcRoot, f), 'utf8');
const portalSources = src('adapters/ticket-portal-sources.ts');
const bridge = src('adapters/native-support-read.ts');
const registry = src('adapters/native_support_read_sql.rs');
const http = src('adapters/service_teams_http.rs');

const kind = 'support_rd_ticket_portal_installed';
const retiredSelect = "SELECT to_regclass('support_ticket_portal_bindings') IS NOT NULL AND to_regclass('support_ticket_portal_acknowledgements') IS NOT NULL installed";
const singleTableSelect = "SELECT to_regclass('support_ticket_portal_bindings') IS NOT NULL installed";

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

describe('Support read-tail r36 ticket-portal installed native kind', () => {
    test('the TS wrapper is a zero-bind bridge call keeping the installed alias and no private SELECT', () => {
        expect(portalSources).toContain("import {nativeSupportReadAll,SUPPORT_RD_TICKET_PORTAL_INSTALLED,SUPPORT_RD_TICKET_PORTAL_TICKET_IMAGE} from './native-support-read';");
        expect(portalSources).toContain(
            'export async function supportTicketPortalInstalled(database:SqlDatabase){return Boolean((await nativeSupportReadAll<{installed:boolean}>(SUPPORT_RD_TICKET_PORTAL_INSTALLED, [], database)).results[0]?.installed);}'
        );
        expect(portalSources).not.toContain(retiredSelect);
        expect(portalSources.match(/supportTicketPortalInstalled\(/g) ?? []).toHaveLength(1);
        expect(retiredSelect.match(/\?/g)).toBeNull();
        expect(retiredSelect).toContain("support_ticket_portal_acknowledgements') IS NOT NULL installed");
    });

    test('the bridge exposes the kind with its fixed 0-bind arity', () => {
        expect(bridge).toContain(`SUPPORT_RD_TICKET_PORTAL_INSTALLED = '${kind}'`);
        expect(bridge).toContain('[SUPPORT_RD_TICKET_PORTAL_INSTALLED]: 0,');
        expect(bridge.match(/SUPPORT_RD_TICKET_PORTAL_INSTALLED/g) ?? []).toHaveLength(2);
    });

    test('the Rust registry owns the exact both-table literal verbatim at 37->38 kinds', () => {
        expect(registry).toContain(`RD_TICKET_PORTAL_INSTALLED: &str = "${kind}"`);
        expect(registry).toContain(`"${retiredSelect}"`);
        expect(registry).toContain('kind: RD_TICKET_PORTAL_INSTALLED,');
        expect(registry).toContain('    READ_TICKET_PORTAL_INSTALLED,\n');
        expect(registry).toContain('assert_eq!(STATEMENTS.len(), 55);');
        expect(registry).toContain('                RD_TICKET_PORTAL_INSTALLED,\n                SQL_TICKET_PORTAL_INSTALLED,\n                0usize,');
        expect(registry).toContain('ticket_portal_installed_read_renders_the_retired_owner_literal_byte_for_byte');
    });

    test('the command-plan route accepts no binds and rejects one for the new kind', () => {
        expect(http).toContain('("support_rd_ticket_portal_installed", json!([]), 200),');
        expect(http).toContain('("support_rd_ticket_portal_installed", json!([1]), 422),');
    });

    test('the single-table portal bindings kind, locks, advisory and assertions stay untouched', () => {
        expect(registry).toContain(`RD_PORTAL_BINDINGS_INSTALLED: &str = "support_rd_portal_bindings_installed"`);
        expect(registry).toContain(`"${singleTableSelect}"`);
        expect(registry).toContain('    READ_PORTAL_BINDINGS_INSTALLED,\n');
        expect(bridge).toContain("SUPPORT_RD_PORTAL_BINDINGS_INSTALLED = 'support_rd_portal_bindings_installed'");
        expect(bridge).toContain('[SUPPORT_RD_PORTAL_BINDINGS_INSTALLED]: 0,');
        expect(portalSources).toContain('LOCK TABLE support_tickets,support_ticket_events');
        expect(src('adapters/ticket-portal-unit.ts')).toContain('pg_advisory_xact_lock');
        expect(registry).not.toMatch(/LOCK TABLE|pg_advisory_xact_lock|workflow_command_assertion/);
        for (const kept of [
            'SUPPORT_RD_TICKET_PORTAL_TICKET_IMAGE',
            'SUPPORT_RD_TICKET_PORTAL_RESPONSE',
            'SUPPORT_RD_TICKET_PORTAL_ACKNOWLEDGED_AT',
            'SUPPORT_RD_CUSTOMER_CONTACT_NOTES',
        ]) expect(bridge).toContain(kept);
        for (const kept of [
            'READ_TICKET_PORTAL_TICKET_IMAGE',
            'READ_TICKET_PORTAL_RESPONSE',
            'READ_TICKET_PORTAL_ACKNOWLEDGED_AT',
            'READ_CUSTOMER_CONTACT_NOTES',
        ]) expect(registry).toContain(`    ${kept},\n`);
    });

    test('retirement:ts-statements drops from 43 to 42', () => {
        expect(selectCount()).toBe(24);
        expect(selectCount()).toBeLessThan(43);
    });
});
