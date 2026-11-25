import { describe, expect, test, vi } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
vi.mock('server-only', () => ({}));
import { settlementConfiguration, createSettlementEventsClient, readSettlementEventsReply } from '../../../packages/server-benefits/src/adapters/native-lane-settlement/client';

const srcRoot = resolve(__dirname, '../../../packages/server-benefits/src');
const read = (name: string) => readFileSync(resolve(srcRoot, name), 'utf8');
const sql = read('adapters/native_settlement_events/postgres.rs');
const source = sql.match(/pub const SOURCE_SQL: &str = "([^"]+)"/)?.[1] ?? '';

// The four-arm event history retired from package-obligations.ts, rendered with
// sqlx positional binds. Table order, projections and ORDER BY are unchanged.
const nativeOracle = "SELECT 'plan' kind,to_jsonb(event)::text image FROM benefit_plan_events event JOIN benefit_plans r ON r.id=event.plan_id WHERE r.company_id=$1 UNION ALL SELECT 'window',to_jsonb(event)::text FROM benefit_window_events event JOIN benefit_enrollment_windows r ON r.id=event.window_id WHERE r.company_id=$1 UNION ALL SELECT 'dependent',to_jsonb(event)::text FROM benefit_dependent_events event JOIN benefit_dependents r ON r.id=event.dependent_id WHERE r.company_id=$1 UNION ALL SELECT 'election',to_jsonb(event)::text FROM benefit_election_events event JOIN benefit_elections r ON r.id=event.election_id WHERE r.company_id=$1 ORDER BY kind,image";

describe('Benefits settlement event-history native route cutover', () => {
  test('the native sqlx statement keeps all four arms and the Company scope', () => {
    expect(source).toBe(nativeOracle);
    expect(source).not.toContain('?');
  });

  test('the TypeScript settlement adapter has no SQL and calls the native route only', () => {
    const adapter = read('adapters/package-obligations.ts');
    expect(adapter).not.toContain('benefitEventHistoryArm');
    expect(adapter).not.toContain('FROM benefit_plan_events');
    expect(adapter).not.toContain('FROM benefit_election_events');
    expect(adapter).toContain('benefitSettlementEvents(input.companyId)');
    const client = read('adapters/native-lane-settlement/client.ts');
    expect(client).not.toMatch(/\bSELECT\b|\bINSERT\b|\bUPDATE\b|\bDELETE\b/);
    expect(client).toContain('/benefits/settlement-events');
  });

  test('the client validates replies and fails closed on invalid configuration', () => {
    expect(readSettlementEventsReply({ events: [{ kind: 'plan', image: '{}' }] })).toEqual([{ kind: 'plan', image: '{}' }]);
    expect(() => readSettlementEventsReply({ events: [{ kind: 'plan', image: 1 }] })).toThrow();
    expect(() => settlementConfiguration(undefined)).toThrow();
  });

  test('the client sends one bounded Company header and no SQL', async () => {
    const calls: Array<{ url: URL; headers: Record<string, string> }> = [];
    const cfg = settlementConfiguration(JSON.stringify({
      privateBaseUrl: 'http://127.0.0.1:3911',
      host: 'acme.example.com',
      holdingSlug: 'acme',
      origin: 'https://acme.example.com',
      timeoutMs: 1000,
    }));
    const client = createSettlementEventsClient(cfg, async (url, headers) => {
      calls.push({ url, headers });
      return { status: 200, value: { events: [{ kind: 'plan', image: '{}' }] } };
    });
    expect(await client('company-1')).toEqual([{ kind: 'plan', image: '{}' }]);
    expect(calls).toHaveLength(1);
    expect(calls[0].url.pathname).toBe('/api/v1/holdings/acme/benefits/settlement-events');
    expect(calls[0].headers['x-erpx-context-company']).toBe('company-1');
    await expect(client('bad company')).rejects.toThrow();
  });
});

describe('Benefits read-tail r13 retirement check', () => {
  test('retirement:ts-statements stays strictly below the r13 BEFORE=57 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT/g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(57);
  });
});
