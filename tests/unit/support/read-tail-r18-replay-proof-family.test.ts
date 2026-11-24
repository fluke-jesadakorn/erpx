import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test, vi } from 'vitest';

vi.mock('server-only', () => ({}));

import { portalReplayProofSql } from '@erpx/server-support/adapters/ticket-portal-sources';

const srcRoot = resolve(__dirname, '../../../packages/server-support/src');
const shared = readFileSync(resolve(srcRoot, 'adapters/ticket-portal-sources.ts'), 'utf8');
const portal = readFileSync(resolve(srcRoot, 'adapters/ticket-portal.ts'), 'utf8');

/** The exact pre-dedupe replay-proof literals from HEAD. */
const decisionBefore = `SELECT 1 FROM support_ticket_portal_events event JOIN command_receipts receipt ON receipt.command_id=event.command_id AND receipt.record_id=event.binding_id AND receipt.actor_id=event.actor_id AND receipt.operation='support.ticket-portal.'||event.action AND receipt.authority_json::jsonb->>'companyId'=event.company_id WHERE event.command_id=? AND event.binding_id=? AND event.company_id=? AND event.actor_id=? AND event.action=? AND receipt.result_json::jsonb=?::jsonb AND receipt.result_json::jsonb=jsonb_build_object('id',event.binding_id,'status',CASE event.action WHEN 'prepare' THEN 'draft' WHEN 'approve' THEN 'approved' WHEN 'reject' THEN 'rejected' ELSE 'revoked' END,'version',event.version)`;

const responseBefore = `SELECT 1 FROM support_ticket_portal_responses response JOIN command_receipts receipt ON receipt.command_id=response.command_id AND receipt.record_id=response.binding_id AND receipt.actor_id=response.actor_id AND receipt.operation='support.ticket-portal.respond' AND receipt.authority_json::jsonb->>'companyId'=response.company_id WHERE response.command_id=? AND response.binding_id=? AND response.company_id=? AND response.actor_id=? AND receipt.result_json::jsonb=?::jsonb AND receipt.result_json::jsonb=jsonb_build_object('id',response.id,'bindingId',response.binding_id,'version',response.response_version,'status','published','message',response.message,'publishedAt',response.created_at)`;

describe('Support read-tail r18 replay-proof family', () => {
    test('renders the decision replay proof byte-identically', () => {
        expect(portalReplayProofSql(
            'support_ticket_portal_events', 'event', `'support.ticket-portal.'||event.action`,
            ' AND event.action=?',
            `jsonb_build_object('id',event.binding_id,'status',CASE event.action WHEN 'prepare' THEN 'draft' WHEN 'approve' THEN 'approved' WHEN 'reject' THEN 'rejected' ELSE 'revoked' END,'version',event.version)`,
        )).toBe(decisionBefore);
    });

    test('renders the response replay proof byte-identically', () => {
        expect(portalReplayProofSql(
            'support_ticket_portal_responses', 'response', `'support.ticket-portal.respond'`,
            '',
            `jsonb_build_object('id',response.id,'bindingId',response.binding_id,'version',response.response_version,'status','published','message',response.message,'publishedAt',response.created_at)`,
        )).toBe(responseBefore);
    });

    test('one shared helper, no local replay-proof SELECT in the caller', () => {
        expect(shared.match(/export const portalReplayProofSql=/g) ?? []).toHaveLength(1);
        expect(shared.match(/JOIN command_receipts receipt ON receipt\.command_id=/g) ?? []).toHaveLength(1);
        expect(portal.match(/portalReplayProofSql\(/g) ?? []).toHaveLength(2);
        expect(portal).not.toMatch(/SELECT 1 FROM support_ticket_portal_events event JOIN command_receipts/);
        expect(portal).not.toMatch(/SELECT 1 FROM support_ticket_portal_responses response JOIN command_receipts/);
    });

    test('retirement:ts-statements stays strictly below the r18 BEFORE=61 baseline', () => {
        let total = 0;
        const walk = (dir: string) => {
            for (const entry of readdirSync(dir, { withFileTypes: true })) {
                const path = resolve(dir, entry.name);
                if (entry.isDirectory()) walk(path);
                else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT/g) ?? []).length;
            }
        };
        walk(srcRoot);
        expect(total).toBeLessThan(61);
    });
});
