import { createServer, type Server } from 'node:http';
import { afterEach, describe, expect, it, vi } from 'vitest';

vi.mock('server-only', () => ({}));

import {
  nativeCommerceReadAll,
  nativeCommerceReadFirst,
  NativeCommerceReadError,
} from '../../../packages/server-commerce/src/adapters/native-commerce-read';
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
import { readFile } from 'node:fs/promises';
import {
  COMMERCE_ACTIVE_WAREHOUSES_SQL,
  COMMERCE_BILLING_TEST_SOURCE_KEY_SQL,
  COMMERCE_EXPENSE_RECEIPT_IMAGE_SQL,
  COMMERCE_MASTER_SOURCE_SQL,
  COMMERCE_MFG_INITIALIZED_GUARD_SQL,
  COMMERCE_OWN_RECEIPT_SOURCE_SQL,
  COMMERCE_POSTING_RECEIPT_RECORD_GUARD_SQL,
  COMMERCE_PRODUCT_ACTIVE_SQL,
  COMMERCE_PRODUCT_COMPANY_ACTIVE_GUARD_SQL,
  COMMERCE_WAREHOUSE_ACTIVE_SQL,
  COMMERCE_WAREHOUSE_BRANCH_ACTIVE_SQL,
  COMMERCE_WAREHOUSE_IDENTITY_SQL,
} from '../../../packages/server-commerce/src/adapters/guarded-source-dedupe-sql';
import { COMMERCE_BRANCH_LABEL_SQL, COMMERCE_FLOW_PARENT_SQL } from '../../../packages/server-commerce/src/adapters/projection-source-dedupe-sql';
import { COMMERCE_STOCK_PRODUCT_FOR_UPDATE_SQL, COMMERCE_STOCK_WAREHOUSE_FOR_UPDATE_SQL } from '../../../packages/server-commerce/src/adapters/stock-source-dedupe-sql';
import { COMMERCE_POSTING_MAPPING_STATE_SQL } from '../../../packages/server-commerce/src/adapters/settings-dedupe-sql';
import { commercialOwnerProjectionSources } from '@erpx/server-inventory/public/commercial-owner-projection-sources';
import { commercialOwnerProjectionSources as financeProjectionSources } from '@erpx/server-finance/public/commercial-owner-projection-sources';

const host = 'acme.example.com';
const origin = `https://${host}`;
const company = 'company';

const config = (privateBaseUrl: string) =>
  JSON.stringify({ privateBaseUrl, host, holdingSlug: 'acme', origin, timeoutMs: 1000 });

let server: Server | undefined;
afterEach(() => {
  server?.close();
  server = undefined;
  delete process.env.ERPX_NATIVE_COMMERCE_OWNER_WRITE_CONFIG;
});

async function listen(
  handler: (req: { headers: Record<string, unknown>; url?: string; body: string }) => {
    status: number;
    value: unknown;
  },
): Promise<string> {
  server = createServer((req, res) => {
    const chunks: Buffer[] = [];
    req.on('data', chunk => chunks.push(Buffer.from(chunk)));
    req.on('end', () => {
      const result = handler({
        headers: req.headers,
        url: req.url,
        body: Buffer.concat(chunks).toString('utf8'),
      });
      res.writeHead(result.status, { 'content-type': 'application/json' });
      res.end(JSON.stringify(result.value));
    });
  });
  await new Promise<void>(resolve => server!.listen(0, '127.0.0.1', resolve));
  const address = server.address();
  if (!address || typeof address === 'string') throw new Error('no listener');
  return `http://127.0.0.1:${address.port}`;
}

type Call = { sql: string; values: unknown[]; first: boolean };
/** Minimal caller unit: records the owner-planned statement it is asked to run. */
function database(calls: Call[], row: unknown = { one: 1 }): SqlDatabase {
  const db: Record<string, unknown> = {
    prepare(sql: string) {
      const call: Call = { sql, values: [], first: false };
      calls.push(call);
      return {
        bind(...values: unknown[]) {
          call.values = values;
          return this;
        },
        async first<T>() {
          call.first = true;
          return row as T;
        },
        async all<T>() {
          return { results: [row] as T[] };
        },
      };
    },
  };
  return db as unknown as SqlDatabase;
}

describe('native Commerce command-plan read transport', () => {
  it('fails closed when the native owner transport is not configured', async () => {
    delete process.env.ERPX_NATIVE_COMMERCE_OWNER_WRITE_CONFIG;
    await expect(
      nativeCommerceReadFirst('commerce_rd_pms_warehouse_active', [company], database([])),
    ).rejects.toMatchObject({ code: 'COMMERCE_READ_UNAVAILABLE', status: 503 });
  });

  it('fails closed on a malformed native owner transport configuration', async () => {
    process.env.ERPX_NATIVE_COMMERCE_OWNER_WRITE_CONFIG = config('http://127.0.0.1:3100');
    await expect(
      nativeCommerceReadFirst('commerce_rd_pms_settings_exists', [company], database([])),
    ).rejects.toBeInstanceOf(NativeCommerceReadError);
  });

  it('sends only kind and values, then runs the owner-planned statement on the caller unit', async () => {
    const sql = "SELECT 1 FROM commerce_warehouses WHERE company_id=? AND status='active' LIMIT 1";
    let request: { headers: Record<string, unknown>; url?: string; body: string } | undefined;
    const base = await listen(req => {
      request = req;
      return { status: 200, value: { data: { statements: [{ sql, values: [company] }] } } };
    });
    process.env.ERPX_NATIVE_COMMERCE_OWNER_WRITE_CONFIG = config(base);
    const calls: Call[] = [];
    const row = { one: 1 };
    await expect(
      nativeCommerceReadFirst('commerce_rd_pms_warehouse_active', [company], database(calls, row)),
    ).resolves.toEqual(row);
    // The wire carries no SQL from the TS side.
    expect(JSON.parse(request!.body)).toEqual({
      kind: 'commerce_rd_pms_warehouse_active',
      values: [company],
    });
    expect(request!.headers).toMatchObject({
      host,
      origin,
      accept: 'application/json',
      'content-type': 'application/json',
    });
    expect(request!.url).toBe('/api/v1/holdings/acme/commerce/command-plan');
    // The owner-planned statement and its binds are executed on the caller unit.
    expect(calls).toEqual([{ sql, values: [company], first: true }]);
  });

  it('supports all-row reads on the caller unit', async () => {
    const sql = 'SELECT 1 FROM commerce_settings WHERE company_id=?';
    const base = await listen(() => ({
      status: 200,
      value: { data: { statements: [{ sql, values: [company] }] } },
    }));
    process.env.ERPX_NATIVE_COMMERCE_OWNER_WRITE_CONFIG = config(base);
    const calls: Call[] = [];
    await expect(
      nativeCommerceReadAll('commerce_rd_pms_settings_exists', [company], database(calls)),
    ).resolves.toEqual([{ one: 1 }]);
    expect(calls).toEqual([{ sql, values: [company], first: false }]);
  });

  it('fails closed on a malformed native plan reply', async () => {
    const base = await listen(() => ({ status: 200, value: { data: { statements: [] } } }));
    process.env.ERPX_NATIVE_COMMERCE_OWNER_WRITE_CONFIG = config(base);
    await expect(
      nativeCommerceReadFirst('commerce_rd_pms_settings_exists', [company], database([])),
    ).rejects.toMatchObject({ code: 'COMMERCE_READ_RESPONSE_INVALID', status: 503 });
  });
});

const adapters = (file: string) =>
  new URL(`../../../packages/server-commerce/src/adapters/${file}`, import.meta.url);

const readTailCases = [
  {
    name: 'COMMERCE_WAREHOUSE_ACTIVE_SQL',
    value: COMMERCE_WAREHOUSE_ACTIVE_SQL,
    literal: "SELECT * FROM commerce_warehouses WHERE id=? AND company_id=? AND status='active' AND (?::text IS NULL OR branch_id=?)",
    files: [
      'commerce.ts',
      'ecommerce-sources.ts',
      'initial-physical-stock-count-sources.ts',
      'inventory-counts-sources.ts',
      'production-command-sources.ts',
    ],
  },
  {
    name: 'COMMERCE_ACTIVE_WAREHOUSES_SQL',
    value: COMMERCE_ACTIVE_WAREHOUSES_SQL,
    literal: "SELECT id,name,branch_id FROM commerce_warehouses WHERE company_id=? AND status='active' AND (?::text IS NULL OR branch_id=?) ORDER BY name",
    files: ['ecommerce-sources.ts', 'inventory-counts-sources.ts', 'production-command-sources.ts'],
  },
  {
    name: 'COMMERCE_PRODUCT_COMPANY_ACTIVE_GUARD_SQL',
    value: COMMERCE_PRODUCT_COMPANY_ACTIVE_GUARD_SQL,
    literal: "EXISTS(SELECT 1 FROM commerce_products WHERE id=? AND company_id=? AND status='active')",
    files: ['production-command-sources.ts'],
  },
  {
    name: 'COMMERCE_WAREHOUSE_IDENTITY_SQL',
    value: COMMERCE_WAREHOUSE_IDENTITY_SQL,
    literal: 'SELECT id,company_id,branch_id,status FROM commerce_warehouses',
    files: ['manufacturing-purchase-sources.ts', 'sales-package-sources.ts', 'supply-package-sources.ts'],
  },
  {
    name: 'COMMERCE_WAREHOUSE_BRANCH_ACTIVE_SQL',
    value: COMMERCE_WAREHOUSE_BRANCH_ACTIVE_SQL,
    literal: "SELECT id,name FROM commerce_warehouses WHERE company_id=? AND branch_id=? AND status='active'",
    files: ['pms-command-facts.ts', 'pms-hospitality-setup-facts.ts'],
  },
  {
    name: 'COMMERCE_PRODUCT_ACTIVE_SQL',
    value: COMMERCE_PRODUCT_ACTIVE_SQL,
    literal: "SELECT * FROM commerce_products WHERE id=? AND company_id=? AND status='active'",
    files: ['commerce.ts', 'inventory-counts-sources.ts'],
  },
] as const;

describe('commerce read-tail r11 warehouse/product guard dedupe', () => {
  it('keeps exactly one byte-identical literal per shared constant', async () => {
    const shared = await readFile(adapters('guarded-source-dedupe-sql.ts'), 'utf8');
    for (const { name, value, literal } of readTailCases) {
      expect(value, name).toBe(literal);
      expect(shared.split(literal).length - 1, name).toBe(1);
    }
    // Pre-existing posting constants, nine r11 literals and three r13 guards.
    expect(shared.split('SELECT').length - 1).toBe(15);
  });

  it('backs every call site with its shared constant and no inline literal', async () => {
    for (const { name, literal, files } of readTailCases) {
      for (const file of files) {
        const text = await readFile(adapters(file), 'utf8');
        expect(text.split(literal).length - 1, `${name} inline in ${file}`).toBe(0);
        expect(text, `${name} import in ${file}`).toContain(name);
      }
    }
  });

  it('shares billing, master, receipt, branch-label and stock-lock text', async () => {
    expect(COMMERCE_BILLING_TEST_SOURCE_KEY_SQL).toBe('SELECT 1 FROM billing_test_finance_entries WHERE source_key=?');
    expect(COMMERCE_OWN_RECEIPT_SOURCE_SQL).toContain('to_jsonb(receipt)::text source FROM command_receipts');
    expect(COMMERCE_MASTER_SOURCE_SQL('commerce_products', false)).toContain('to_jsonb(master)::text source FROM commerce_products');
    expect(COMMERCE_MASTER_SOURCE_SQL('commerce_warehouses', true)).toContain('AND (?::text IS NULL OR branch_id=?)');
    expect(COMMERCE_BRANCH_LABEL_SQL).toContain('COALESCE((SELECT n.display_name');
    expect(COMMERCE_STOCK_PRODUCT_FOR_UPDATE_SQL).toContain('SELECT product_id FROM (');
    expect(COMMERCE_STOCK_PRODUCT_FOR_UPDATE_SQL).toContain('FOR UPDATE');

    const files = [
      'billing-test-finance.ts',
      'manufacturing-initial-physical-stock-masters.ts',
      'pos-startup-masters.ts',
      'executive-metric-projections.ts',
      'pos-core-facts.ts',
      'inventory-consumption-sources.ts',
    ];
    for (const file of files) {
      const text = await readFile(adapters(file), 'utf8');
      expect(text.split('SELECT 1 FROM billing_test_finance_entries').length - 1, file).toBe(0);
      expect(text.split('to_jsonb(master)::text source').length - 1, file).toBe(0);
      expect(text.split('to_jsonb(receipt)::text source').length - 1, file).toBe(0);
      expect(text.split('COALESCE((SELECT n.display_name').length - 1, file).toBe(0);
      expect(text.split('SELECT product_id FROM (').length - 1, file).toBe(0);
    }
  });

  it('reuses the posting mapping state in all four posting setups and drops the zero-importer adapter', async () => {
    expect(COMMERCE_POSTING_MAPPING_STATE_SQL).toContain('chart_id "chartId"');
    expect(COMMERCE_POSTING_MAPPING_STATE_SQL).toContain('FROM commerce_settings WHERE company_id=?');
    for (const file of [
      'posting-setup.ts',
      'sales-goods-posting-setup.ts',
      'manufacturing-posting-setup.ts',
      'pos-posting-setup.ts',
    ]) {
      const text = await readFile(adapters(file), 'utf8');
      expect(text, file).toContain('COMMERCE_POSTING_MAPPING_STATE_SQL');
      expect(text.split('SELECT ${fields.map(').length - 1, file).toBe(0);
    }
    await expect(readFile(adapters('finance-supplier-payment-source-facts.ts'), 'utf8')).rejects.toThrow();
  });
});

describe('commerce read-tail r12 stock warehouse lock dedupe', () => {
  const lockLiteral = 'SELECT warehouse_id FROM (';

  it('keeps one byte-identical FOR UPDATE literal behind the shared constant', async () => {
    // Runtime value is byte-identical to the two original inline literals.
    expect(COMMERCE_STOCK_WAREHOUSE_FOR_UPDATE_SQL).toBe(
      `SELECT warehouse_id FROM (${commercialOwnerProjectionSources.commerce_stock}) commerce_stock WHERE warehouse_id=? AND product_id=? FOR UPDATE`,
    );
    const shared = await readFile(adapters('stock-source-dedupe-sql.ts'), 'utf8');
    expect(shared.split(lockLiteral).length - 1).toBe(1);
    expect(COMMERCE_STOCK_WAREHOUSE_FOR_UPDATE_SQL).toContain('FOR UPDATE');
  });

  it('backs both lock call sites with the constant and no inline literal or shared statement', async () => {
    const text = await readFile(adapters('production-command-sources.ts'), 'utf8');
    expect(text.split(lockLiteral).length - 1).toBe(0);
    expect(text.split('COMMERCE_STOCK_WAREHOUSE_FOR_UPDATE_SQL').length - 1).toBe(3); // import + 2 prepares
    expect(text).toContain('lockReserveStock');
    expect(text).toContain('lockIssueStock');
    // Every site still prepares its own statement in its own transaction.
    expect(text).toContain('db.prepare(COMMERCE_STOCK_WAREHOUSE_FOR_UPDATE_SQL)');
  });
});

describe('commerce read-tail r13 posting/flow guard dedupe', () => {
  const cases = [
    {
      name: 'COMMERCE_FLOW_PARENT_SQL',
      value: COMMERCE_FLOW_PARENT_SQL,
      literal: `SELECT f.order_id FROM commerce_fulfillments f JOIN (${financeProjectionSources.finance_journals}) j ON j.id=f.journal_id WHERE j.id=? AND j.company_id=?`,
      file: 'workflow-flow-routing-facts.ts',
      prepares: 2,
    },
    {
      name: 'COMMERCE_EXPENSE_RECEIPT_IMAGE_SQL',
      value: COMMERCE_EXPENSE_RECEIPT_IMAGE_SQL,
      literal: 'SELECT to_jsonb(r)::text image FROM command_receipts r WHERE r.command_id=?',
      file: 'expense-posting-setup.ts',
      prepares: 2,
    },
    {
      name: 'COMMERCE_MFG_INITIALIZED_GUARD_SQL',
      value: COMMERCE_MFG_INITIALIZED_GUARD_SQL,
      literal: "SELECT 1 FROM command_receipts WHERE operation='commerce.manufacturing-posting-setup.initialize' AND record_id=? AND authority_json::jsonb->>'companyId'=?",
      file: 'manufacturing-existing-posting-setup.ts',
      prepares: 2,
    },
  ] as const;

  it('keeps one byte-identical literal per shared constant', () => {
    for (const { name, value, literal } of cases) expect(value, name).toBe(literal);
    expect(COMMERCE_POSTING_RECEIPT_RECORD_GUARD_SQL).toBe(
      "SELECT 1 FROM command_receipts WHERE command_id=? AND operation=? AND record_id=? AND actor_id=? AND authority_json::jsonb->>'companyId'=?",
    );
  });

  it('backs every call site with the constant and no inline literal', async () => {
    for (const { name, literal, file, prepares } of cases) {
      const text = await readFile(adapters(file), 'utf8');
      for (const quote of ["'", '"', '`']) {
        expect(text.split(`database.prepare(${quote}${literal}${quote})`).length - 1, `${name} inline in ${file}`).toBe(0);
      }
      expect(text.split(`database.prepare(${name})`).length - 1, `${name} prepares in ${file}`).toBe(prepares);
    }
    for (const file of ['manufacturing-posting-setup.ts', 'pos-posting-setup.ts']) {
      const text = await readFile(adapters(file), 'utf8');
      expect(text.split(COMMERCE_POSTING_RECEIPT_RECORD_GUARD_SQL).length - 1, file).toBe(0);
      expect(text.split('database.prepare(COMMERCE_POSTING_RECEIPT_RECORD_GUARD_SQL)').length - 1, file).toBe(1);
    }
  });
});
