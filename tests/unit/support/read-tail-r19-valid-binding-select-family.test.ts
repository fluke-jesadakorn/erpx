import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test, vi } from 'vitest';

vi.mock('server-only', () => ({}));

import { portalValidBindingSelect, supportTicketPortalValidBindings } from '@erpx/server-support/adapters/ticket-portal-sources';

const srcRoot = resolve(__dirname, '../../../packages/server-support/src');
const sources = readFileSync(resolve(srcRoot, 'adapters/ticket-portal-sources.ts'), 'utf8');
const staff = readFileSync(resolve(srcRoot, 'adapters/ticket-portal.ts'), 'utf8');
const customer = readFileSync(resolve(srcRoot, 'adapters/ticket-portal-customer.ts'), 'utf8');

/** The exact pre-dedupe outer read prefix from HEAD, composed over the unchanged
 * reviewed valid-binding source. */
const before = `SELECT binding.* FROM (${supportTicketPortalValidBindings}) binding`;

describe('Support read-tail r19 valid-binding outer read family', () => {
    test('renders the outer read prefix byte-identically', () => {
        expect(portalValidBindingSelect).toBe(before);
        expect(portalValidBindingSelect).toContain('SELECT binding.* FROM (');
    });

    test('one shared helper, no inlined prefix at either caller', () => {
        expect(sources.match(/export const portalValidBindingSelect=/g) ?? []).toHaveLength(1);
        expect((staff.match(/portalValidBindingSelect/g) ?? []).length).toBeGreaterThanOrEqual(1);
        expect((customer.match(/portalValidBindingSelect/g) ?? []).length).toBeGreaterThanOrEqual(1);
        expect(staff).not.toContain('SELECT binding.* FROM (${supportTicketPortalValidBindings}) binding ');
        expect(customer).not.toContain('SELECT binding.* FROM (${supportTicketPortalValidBindings}) binding ');
    });

    test('retirement:ts-statements stays strictly below the r19 BEFORE=60 baseline', () => {
        let total = 0;
        const walk = (dir: string) => {
            for (const entry of readdirSync(dir, { withFileTypes: true })) {
                const path = resolve(dir, entry.name);
                if (entry.isDirectory()) walk(path);
                else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT/g) ?? []).length;
            }
        };
        walk(srcRoot);
        expect(total).toBeLessThan(60);
    });
});
