import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test, vi } from 'vitest';

vi.mock('server-only', () => ({}));

const srcRoot = resolve(__dirname, '../../../packages/server-support/src');
const src = (p: string) => readFileSync(resolve(srcRoot, p), 'utf8');
const shared = src('adapters/ticket-portal-sources.ts');
const portal = src('adapters/ticket-portal.ts');
const bridge = src('adapters/native-support-read.ts');
const registry = src('adapters/native_support_read_sql.rs');

/** Exact pre-r18 receipt-proof literals from HEAD; the native registry now
 * carries both byte-for-byte and the TypeScript renderer is gone. */
const decisionBefore = `SELECT 1 FROM support_ticket_portal_events event JOIN command_receipts receipt ON receipt.command_id=event.command_id AND receipt.record_id=event.binding_id AND receipt.actor_id=event.actor_id AND receipt.operation='support.ticket-portal.'||event.action AND receipt.authority_json::jsonb->>'companyId'=event.company_id WHERE event.command_id=? AND event.binding_id=? AND event.company_id=? AND event.actor_id=? AND event.action=? AND receipt.result_json::jsonb=?::jsonb AND receipt.result_json::jsonb=jsonb_build_object('id',event.binding_id,'status',CASE event.action WHEN 'prepare' THEN 'draft' WHEN 'approve' THEN 'approved' WHEN 'reject' THEN 'rejected' ELSE 'revoked' END,'version',event.version)`;

const responseBefore = `SELECT 1 FROM support_ticket_portal_responses response JOIN command_receipts receipt ON receipt.command_id=response.command_id AND receipt.record_id=response.binding_id AND receipt.actor_id=response.actor_id AND receipt.operation='support.ticket-portal.respond' AND receipt.authority_json::jsonb->>'companyId'=response.company_id WHERE response.command_id=? AND response.binding_id=? AND response.company_id=? AND response.actor_id=? AND receipt.result_json::jsonb=?::jsonb AND receipt.result_json::jsonb=jsonb_build_object('id',response.id,'bindingId',response.binding_id,'version',response.response_version,'status','published','message',response.message,'publishedAt',response.created_at)`;

function rustLiteral(name: string): string {
    const match = registry.match(new RegExp(`const SQL_${name}: &str =\\s*("(?:[^"\\\\]|\\\\.)*");`));
    if (!match) throw new Error(`missing SQL_${name}`);
    return JSON.parse(match[1]!) as string;
}

describe('Support read-tail r18 replay-proof family', () => {
    test('both retired receipt-proof literals are inlined byte-for-byte in Rust', () => {
        expect(rustLiteral('TICKET_PORTAL_DECISION_REPLAY')).toBe(decisionBefore);
        expect(rustLiteral('TICKET_PORTAL_RESPONSE_REPLAY')).toBe(responseBefore);
        expect(decisionBefore.match(/\?/g)).toHaveLength(6);
        expect(responseBefore.match(/\?/g)).toHaveLength(5);
        expect(registry).toContain('pub const RD_TICKET_PORTAL_DECISION_REPLAY: &str = "support_rd_ticket_portal_decision_replay";');
        expect(registry).toContain('pub const RD_TICKET_PORTAL_RESPONSE_REPLAY: &str = "support_rd_ticket_portal_response_replay";');
        expect(registry).toContain('(RD_TICKET_PORTAL_DECISION_REPLAY, SQL_TICKET_PORTAL_DECISION_REPLAY, 6usize)');
        expect(registry).toContain('(RD_TICKET_PORTAL_RESPONSE_REPLAY, SQL_TICKET_PORTAL_RESPONSE_REPLAY, 5usize)');
        for (const sql of [decisionBefore, responseBefore]) {
            expect(sql).not.toMatch(/INSERT|UPDATE|DELETE|FOR UPDATE|FOR SHARE|LOCK TABLE/i);
            expect(sql).not.toContain(';');
        }
    });

    test('one native owner kind per proof and the bridge carries fixed arity', () => {
        expect(bridge).toContain("SUPPORT_RD_TICKET_PORTAL_DECISION_REPLAY = 'support_rd_ticket_portal_decision_replay'");
        expect(bridge).toContain("SUPPORT_RD_TICKET_PORTAL_RESPONSE_REPLAY = 'support_rd_ticket_portal_response_replay'");
        expect(bridge).toContain('[SUPPORT_RD_TICKET_PORTAL_DECISION_REPLAY]: 6');
        expect(bridge).toContain('[SUPPORT_RD_TICKET_PORTAL_RESPONSE_REPLAY]: 5');
    });

    test('the TypeScript renderer is deleted and both proofs route natively', () => {
        expect(shared).not.toContain('portalReplayProofSql');
        expect(shared).not.toContain('JOIN command_receipts receipt ON receipt.command_id=');
        expect(portal).not.toContain('portalReplayProofSql');
        expect(portal).not.toMatch(/database\.prepare\(.*command_receipts/);
        expect(portal).toContain('nativeSupportReadFirst(SUPPORT_RD_TICKET_PORTAL_DECISION_REPLAY,[key,bindingId,actor.companyId,actor.userId,action,JSON.stringify(result)],database)');
        expect(portal).toContain('nativeSupportReadFirst(SUPPORT_RD_TICKET_PORTAL_RESPONSE_REPLAY,[key,bindingId,actor.companyId,actor.userId,JSON.stringify(result)],database)');
        expect(portal).toContain("if(!ok)throw ticketPortalChanged();");
    });

    test('retirement:ts-statements drops from the r47 AFTER=27 to 26', () => {
        let total = 0;
        const walk = (dir: string) => {
            for (const entry of readdirSync(dir, { withFileTypes: true })) {
                const path = resolve(dir, entry.name);
                if (entry.isDirectory()) walk(path);
                else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
            }
        };
        walk(srcRoot);
        expect(total).toBe(24);
        expect(total).toBeLessThan(27);
    });
});
