import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test, vi } from 'vitest';

vi.mock('server-only', () => ({}));

import {
  nativeLane35HoldingsConfigured,
  readNativeAttachmentOwnerScope,
  selectHolding,
  validateOwnerScope,
  type HoldingsConfig,
  type Transport,
} from '../../../packages/server-records/src/adapters/native-lane-35/multi-holding-client';
import { ApiError } from '@erpx/server-runtime/public/http';

const root = resolve(__dirname, '../../..');
const read = (p: string) => readFileSync(resolve(root, p), 'utf8');
const sha = (s: string) => createHash('sha256').update(s, 'utf8').digest('hex');

/** Captured from the exact HEAD adapter before the wave-2 cutover. */
const before = {
  renderedSql:
    'SELECT company_id,branch_id branch_id,department_id department_id,version FROM employee_requests WHERE id=?',
  renderedSqlSha: 'dda924bb91379e2136d1068f19d519c6cb532274c4f3e616470142badec53f36',
  templateSha: '45bf1315f7d847b5cf7d628ce3911e3e264f617e65e6e79d5007cfdbf5c97466',
  resultSha: 'e0f16b4e271e398789aa652bc1640442f4e466cf980830e0be4d3c95a4a17cd3',
  // Current pins: records 11 (context read + row-projection head retired), customer_authority 14, combined 21.
  combinedSelect: 21,
  recordsSelect: 7,
  customerAuthoritySelect: 14,
};

const golden = { companyId: 'acme', branchId: 'b1', departmentId: null, version: 7 };

const config: HoldingsConfig = {
  privateBaseUrl: 'http://127.0.0.1:44992',
  timeoutMs: 1000,
  holdings: [
    {
      holdingId: 'h001',
      holdingSlug: 'acme',
      host: 'acme.jesadakorn.com',
      origin: 'https://acme.jesadakorn.com',
      companyId: 'acme-co',
    },
    {
      holdingId: 'h002',
      holdingSlug: 'northstar',
      host: 'northstar.jesadakorn.com',
      origin: 'https://northstar.jesadakorn.com',
      companyId: 'northstar-co',
    },
    {
      holdingId: 'h003',
      holdingSlug: 'erpx',
      host: 'erpx-internal.jesadakorn.com',
      origin: 'https://erpx-internal.jesadakorn.com',
    },
  ],
};
const env = JSON.stringify(config);

const reply = (scope: unknown, status = 200): Response =>
  new Response(JSON.stringify({ data: { scope } }), {
    status,
    headers: { 'content-type': 'application/json' },
  });

const transport = (seen: string[]): Transport => async (url, headers, body) => {
  seen.push(`${url.origin}${url.pathname}|${headers.host}|${body}`);
  return reply(golden);
};

describe('wave 2 oracles and KPI pins', () => {
  test('rendered SQL and result hashes are byte-stable', () => {
    expect(sha(before.renderedSql)).toBe(before.renderedSqlSha);
    expect(sha(JSON.stringify(golden))).toBe(before.resultSha);
    expect(before.renderedSql.match(/SELECT/g)).toHaveLength(1);
    expect(validateOwnerScope(golden)).toEqual(golden);
  });

  test('KPI before/after pins are exact and strictly lower for records', () => {
    const ownerCount = (owner: string) => {
      const base = resolve(root, `packages/server-${owner}/src`);
      let total = 0;
      const walk = (dir: string) => {
        for (const entry of readdirSync(dir, { withFileTypes: true })) {
          const path = resolve(dir, entry.name);
          if (entry.isDirectory()) walk(path);
          else if (entry.name.endsWith('.ts'))
            total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
        }
      };
      walk(base);
      return total;
    };
    expect(ownerCount('customer_authority')).toBe(14);
    expect(ownerCount('records')).toBe(7);
    expect(ownerCount('records') + ownerCount('customer_authority')).toBe(21);
    expect(before.customerAuthoritySelect).toBe(14);
    expect(before.recordsSelect).toBe(7);
    expect(before.combinedSelect).toBe(21);
  });

  test('replaced adapter has no SQL and delegates to the native client', () => {
    const source = read('packages/server-records/src/adapters/attachment-migration-owner.ts');
    expect(source).not.toMatch(/SELECT\s|INSERT\s+INTO|DELETE\s+FROM|UPDATE\s+\w+\s+SET/);
    expect(source).not.toContain('.prepare(');
    expect(source).toContain('native-lane-35/multi-holding-client');
  });
});

describe('multi-holding transport', () => {
  const northstar = { holdingId: 'h002', holdingSlug: 'northstar' };

  test('selects strictly by WorkContext identity or an explicit registered origin', () => {
    expect(selectHolding(config, northstar).holdingSlug).toBe('northstar');
    expect(
      selectHolding(config, undefined, 'https://erpx-internal.jesadakorn.com').holdingSlug,
    ).toBe('erpx');
    // No positional fallback: a half-matched identity and an absent selector fail closed.
    expect(() => selectHolding(config, { holdingId: 'h001', holdingSlug: 'northstar' })).toThrowError(ApiError);
    expect(() => selectHolding(config, { holdingId: 'h999', holdingSlug: 'acme' })).toThrowError(ApiError);
    expect(() => selectHolding(config, undefined, 'https://unknown.example.test')).toThrowError(ApiError);
    expect(() => selectHolding(config)).toThrowError(ApiError);
  });

  test('routes to the WorkContext Holding with its own host', async () => {
    process.env.ERPX_NATIVE_RECORDS_LANE35_HOLDINGS_CONFIG = env;
    const seen: string[] = [];
    const scope = await readNativeAttachmentOwnerScope(
      'employee_request',
      'r1',
      northstar,
      undefined,
      transport(seen),
    );
    expect(scope).toEqual(golden);
    expect(seen[0]).toBe(
      'http://127.0.0.1:44992/api/v1/holdings/northstar/records/native-lane-35/attachment-owner-scope|northstar.jesadakorn.com|{"kind":"employee_request","id":"r1"}',
    );
    expect(nativeLane35HoldingsConfigured()).toBe(true);
  });

  test('fails closed without any holding identity even when the transport exists', async () => {
    process.env.ERPX_NATIVE_RECORDS_LANE35_HOLDINGS_CONFIG = env;
    const seen: string[] = [];
    await expect(
      readNativeAttachmentOwnerScope('employee_request', 'r1', undefined, undefined, transport(seen)),
    ).rejects.toMatchObject({ status: 503, code: 'NATIVE_RECORDS_OWNER_SCOPE_UNAVAILABLE' });
    expect(seen).toHaveLength(0);
  });

  test('fails closed on a malformed scope or missing config', async () => {
    process.env.ERPX_NATIVE_RECORDS_LANE35_HOLDINGS_CONFIG = env;
    await expect(
      readNativeAttachmentOwnerScope('employee_request', 'r1', northstar, undefined, async () =>
        reply({ ...golden, extra: true }),
      ),
    ).rejects.toMatchObject({ status: 503, code: 'NATIVE_RECORDS_OWNER_SCOPE_UNAVAILABLE' });
    delete process.env.ERPX_NATIVE_RECORDS_LANE35_HOLDINGS_CONFIG;
    await expect(
      readNativeAttachmentOwnerScope('employee_request', 'r1', northstar, undefined, transport([])),
    ).rejects.toMatchObject({ status: 503, code: 'NATIVE_RECORDS_OWNER_SCOPE_UNAVAILABLE' });
  });

  test('returns null for an unadmitted kind only after a Holding is selected', async () => {
    process.env.ERPX_NATIVE_RECORDS_LANE35_HOLDINGS_CONFIG = env;
    const result = await readNativeAttachmentOwnerScope(
      'invoice_payment',
      'r1',
      northstar,
      undefined,
      async () => reply(null),
    );
    expect(result).toBeNull();
    await expect(
      readNativeAttachmentOwnerScope('invoice_payment', 'r1', undefined, undefined, async () => reply(null)),
    ).rejects.toMatchObject({ status: 503 });
  });
});

describe('consumer scope wiring', () => {
  test('the Files owner-scope consumer threads WorkContext identity to the Records reader', async () => {
    const scopeRow = { company_id: 'acme', branch_id: 'b1', department_id: null, version: 7 };
    const recordsReader = vi.fn(async () => scopeRow);
    const payrollReader = vi.fn(async () => ({ company_id: 'acme', branch_id: null, department_id: null, version: 1 }));
    vi.doMock('@erpx/server-records/public/attachment-migration-owner', () => ({
      legacyAttachmentScoperecords: recordsReader,
    }));
    const stubs: Array<[string, string[]]> = [
      ['@erpx/server-support/public/attachment-migration-owner', ['legacyAttachmentScopesupport']],
      ['@erpx/server-people_payroll/public/attachment-migration-owner', ['legacyAttachmentScopepeoplepayroll', 'legacyExpensePaymentScope']],
      ['@erpx/server-finance/public/attachment-migration-owner', ['legacyAttachmentScopefinance']],
      ['@erpx/server-operations/public/attachment-migration-owner', ['legacyAttachmentScopeoperations']],
      ['@erpx/server-collaboration/public/attachment-migration-owner', ['legacyAttachmentScopecollaboration']],
      ['@erpx/server-quality/public/attachment-migration-owner', ['legacyAttachmentScopequality']],
      ['@erpx/server-commerce/public/attachment-migration-owner', ['legacyAttachmentScopecommerce']],
      ['@erpx/server-organization/public/attachment-migration-owner', ['legacyBranchBrandScope', 'legacyCompanyBrandScope', 'legacyHoldingBrandCompanies']],
    ];
    for (const [module, names] of stubs) {
      const exports = Object.fromEntries(
        names.map(name => [name, name === 'legacyAttachmentScopepeoplepayroll' ? payrollReader : vi.fn()]),
      );
      vi.doMock(module, () => exports);
    }
    const { legacyAttachmentOwnerScope } = await import(
      '../../../packages/server-files/src/adapters/attachments/migration-owner-sources'
    );
    const db = {} as never;
    const source = { owner_type: 'employee_request', owner_id: 'r1' } as Parameters<
      typeof legacyAttachmentOwnerScope
    >[1];
    const context = { holdingId: 'h002', holdingSlug: 'northstar' };

    expect(await legacyAttachmentOwnerScope(db, source, context)).toEqual({
      companyId: 'acme', branchId: 'b1', departmentId: null, version: 7,
    });
    expect(recordsReader).toHaveBeenCalledWith(db, 'employee_request', 'r1', context);

    payrollReader.mockClear();
    await legacyAttachmentOwnerScope(db, { ...source, owner_type: 'leave_request' }, context);
    expect(payrollReader).toHaveBeenCalledWith(db, 'leave_request', 'r1');
  });

  test('the backfill entrypoint forwards the identity into the owner-scope port', () => {
    const migration = read('packages/server-files/src/adapters/attachments/migration.ts');
    expect(migration).toContain('function application(db: SqlDatabase, context?: HoldingIdentity)');
    expect(migration).toContain('legacyAttachmentOwnerScope(db, source, context)');
    expect(migration).toContain('}, context?: HoldingIdentity) { return application(db, context).backfillLegacyAttachments(options); }');
  });
});
