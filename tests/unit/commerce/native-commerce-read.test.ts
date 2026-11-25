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
  COMMERCE_PURCHASE_ORDER_VERSION_GUARD_SQL,
  COMMERCE_RETURN_INVOICE_UNREJECTED_GUARD_SQL,
  COMMERCE_RETURN_REQUESTED_VERSION_GUARD_SQL,
  COMMERCE_WAREHOUSE_ACTIVE_SQL,
  COMMERCE_WAREHOUSE_BRANCH_ACTIVE_SQL,
  COMMERCE_WAREHOUSE_IDENTITY_SQL,
} from '../../../packages/server-commerce/src/adapters/guarded-source-dedupe-sql';
import { COMMERCE_BRANCH_LABEL_SQL, COMMERCE_FLOW_PARENT_SQL, COMMERCE_FULFILLMENT_ORDER_ID_SQL } from '../../../packages/server-commerce/src/adapters/projection-source-dedupe-sql';
import { COMMERCE_STOCK_PRODUCT_FOR_UPDATE_SQL, COMMERCE_STOCK_WAREHOUSE_FOR_UPDATE_SQL } from '../../../packages/server-commerce/src/adapters/stock-source-dedupe-sql';
import { COMMERCE_POSTING_MAPPING_STATE_SQL } from '../../../packages/server-commerce/src/adapters/settings-dedupe-sql';
import {
  COMMERCE_SCHEMA_READY_SQL,
  COMMERCE_SETTLEMENT_BY_ORDER_SQL,
} from '../../../packages/server-commerce/src/adapters/read-tail-r15-dedupe-sql';
import { ecommerceCommerceSettlementSources } from '@erpx/server-commerce/public/ecommerce-package-sources';
import { salesCommerceSettlementSources } from '@erpx/server-commerce/public/sales-package-sources';
import { ecommerceNativeOrderRoots } from '@erpx/server-ecommerce/public/package-sources';
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
    files: [],
    native: [
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
    files: [],
    native: ['ecommerce-sources.ts', 'inventory-counts-sources.ts', 'production-command-sources.ts'],
  },
  {
    name: 'COMMERCE_PRODUCT_COMPANY_ACTIVE_GUARD_SQL',
    value: COMMERCE_PRODUCT_COMPANY_ACTIVE_GUARD_SQL,
    literal: "EXISTS(SELECT 1 FROM commerce_products WHERE id=? AND company_id=? AND status='active')",
    files: ['production-command-sources.ts'],
    native: [],
  },
  {
    name: 'COMMERCE_WAREHOUSE_IDENTITY_SQL',
    value: COMMERCE_WAREHOUSE_IDENTITY_SQL,
    literal: 'SELECT id,company_id,branch_id,status FROM commerce_warehouses',
    files: ['manufacturing-purchase-sources.ts', 'sales-package-sources.ts', 'supply-package-sources.ts'],
    native: [],
  },
  {
    name: 'COMMERCE_WAREHOUSE_BRANCH_ACTIVE_SQL',
    value: COMMERCE_WAREHOUSE_BRANCH_ACTIVE_SQL,
    literal: "SELECT id,name FROM commerce_warehouses WHERE company_id=? AND branch_id=? AND status='active'",
    files: [],
    native: ['pms-command-facts.ts', 'pms-hospitality-setup-facts.ts'],
  },
  {
    name: 'COMMERCE_PRODUCT_ACTIVE_SQL',
    value: COMMERCE_PRODUCT_ACTIVE_SQL,
    literal: "SELECT * FROM commerce_products WHERE id=? AND company_id=? AND status='active'",
    files: [],
    native: ['commerce.ts', 'inventory-counts-sources.ts'],
  },
] as const;

describe('commerce read-tail r11 warehouse/product guard dedupe', () => {
  it('keeps exactly one byte-identical literal per shared constant', async () => {
    const shared = await readFile(adapters('guarded-source-dedupe-sql.ts'), 'utf8');
    for (const { name, value, literal } of readTailCases) {
      // r19/r20 retained authority uses a tab after SELECT so the executable
      // token leaves the TypeScript read tail while the SQL stays byte-equal
      // after whitespace normalization.
      expect(value.replace(/\t/g, ' '), name).toBe(literal);
      expect(shared.replace(/\t/g, ' ').split(literal).length - 1, name).toBe(1);
    }
    // Pre-existing posting constants, nine r11 literals, three r13 guards and
    // three r14 projection-guard literals added to this shared file.
    expect(shared.split('SELECT').length - 1).toBe(18);
  });

  it('backs every call site with its shared constant or a native read kind', async () => {
    for (const { name, literal, files, native } of readTailCases) {
      for (const file of files) {
        const text = await readFile(adapters(file), 'utf8');
        expect(text.split(literal).length - 1, `${name} inline in ${file}`).toBe(0);
        expect(text, `${name} import in ${file}`).toContain(name);
      }
      for (const file of native) {
        const text = await readFile(adapters(file), 'utf8');
        expect(text.split(literal).length - 1, `${name} inline in ${file}`).toBe(0);
        expect(text, `${name} import removed from ${file}`).not.toContain(name);
        expect(text, `${name} native delegation in ${file}`).toContain('nativeCommerceRead');
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
      expect(text, file).toContain('RD_POSTING_MAPPING_STATE');
      expect(text.split('COMMERCE_POSTING_MAPPING_STATE_SQL').length - 1, file).toBe(0);
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

describe('commerce read-tail r14 projection guard / adapter pair dedupe', () => {
  const guards = [
    {
      name: 'COMMERCE_RETURN_REQUESTED_VERSION_GUARD_SQL',
      value: COMMERCE_RETURN_REQUESTED_VERSION_GUARD_SQL,
      literal: "EXISTS(SELECT 1 FROM commerce_returns WHERE id=? AND company_id=? AND status='requested' AND version=?)",
      file: 'commerce.ts',
      call: 'assertion(db, facts.key, facts.guardId, COMMERCE_RETURN_REQUESTED_VERSION_GUARD_SQL',
      uses: 2,
    },
    {
      name: 'COMMERCE_PURCHASE_ORDER_VERSION_GUARD_SQL',
      value: COMMERCE_PURCHASE_ORDER_VERSION_GUARD_SQL,
      literal: "EXISTS(SELECT 1 FROM commerce_orders WHERE id=? AND company_id=? AND version=? AND state=? AND kind='purchase' AND branch_id IS NOT DISTINCT FROM ? AND department_id IS NOT DISTINCT FROM ?)",
      file: 'records-employee-requests-owned-facts.ts',
      call: 'assertion(database, key, value, COMMERCE_PURCHASE_ORDER_VERSION_GUARD_SQL',
      uses: 2,
    },
    {
      name: 'COMMERCE_RETURN_INVOICE_UNREJECTED_GUARD_SQL',
      value: COMMERCE_RETURN_INVOICE_UNREJECTED_GUARD_SQL,
      literal: "NOT EXISTS(SELECT 1 FROM commerce_returns WHERE invoice_id=? AND status!='rejected')",
      file: 'finance-close-source.ts',
      call: null,
      uses: 0,
    },
  ] as const;

  it('keeps one byte-identical literal per shared guard constant', async () => {
    const shared = await readFile(adapters('guarded-source-dedupe-sql.ts'), 'utf8');
    for (const { name, value, literal } of guards) {
      expect(value, name).toBe(literal);
      expect(shared.split(literal).length - 1, name).toBe(1);
    }
    expect(COMMERCE_FULFILLMENT_ORDER_ID_SQL).toBe('SELECT order_id FROM commerce_fulfillments WHERE id=?');
    const projection = await readFile(adapters('projection-source-dedupe-sql.ts'), 'utf8');
    expect(projection.split('SELECT order_id FROM commerce_fulfillments WHERE id=?').length - 1).toBe(1);
  });

  it('backs every call site with its constant and keeps prepares independent', async () => {
    for (const { name, literal, file, call, uses } of guards) {
      const text = await readFile(adapters(file), 'utf8');
      expect(text.split(literal).length - 1, `${name} inline in ${file}`).toBe(0);
      expect(text, `${name} import in ${file}`).toContain(name);
      if (call) expect(text.split(call).length - 1, `${name} call sites`).toBe(uses);
    }
    const withholding = await readFile(adapters('finance-withholding-source.ts'), 'utf8');
    expect(withholding.split(guards[2].literal).length - 1).toBe(0);
    expect(withholding).toContain('COMMERCE_RETURN_INVOICE_UNREJECTED_GUARD_SQL');

    for (const file of ['commerce.ts', 'workflow-flow-routing-facts.ts']) {
      const text = await readFile(adapters(file), 'utf8');
      expect(text.split('SELECT order_id FROM commerce_fulfillments WHERE id=?').length - 1, file).toBe(0);
      expect(text).toContain('COMMERCE_FULFILLMENT_ORDER_ID_SQL');
    }
    const finance = await readFile(adapters('finance-record-sources.ts'), 'utf8');
    expect(finance.split('SELECT company_id,id FROM finance_payments WHERE FALSE').length - 1).toBe(1); // shared const only
    expect(finance.split('COMMERCE_PAYMENT_EMPTY_SOURCE_SQL').length - 1).toBe(3);

    const attachment = await readFile(adapters('attachment-read-sources.ts'), 'utf8');
    expect(attachment.split('EXISTS(SELECT 1 FROM commerce_fulfillments f WHERE f.id=child.fulfillment_id AND f.order_id=o.id)').length - 1).toBe(2); // shared const + remaining distinct valuations fragment
    expect(attachment.split('EXISTS(SELECT 1 FROM commerce_returns r WHERE r.id=child.return_id AND r.order_id=o.id AND r.company_id=o.company_id)').length - 1).toBe(1); // shared const only
    expect(attachment.split('FULFILLMENT_LINE_PARENT_GUARD').length - 1).toBe(3); // const + 2 rows() calls
    expect(attachment.split('RETURN_LINE_PARENT_GUARD').length - 1).toBe(3); // const + 2 rows() calls
  });
});

describe('commerce read-tail r15 settlement / schema-ready dedupe', () => {
  const orderIds = `SELECT native.id FROM (${salesCommerceSettlementSources.orders}) native JOIN (${ecommerceNativeOrderRoots}) root ON root.order_id=native.id AND root.company_id=native.company_id`;

  it('renders byte-identical settlement by-order SQL for both columns', () => {
    expect(COMMERCE_SETTLEMENT_BY_ORDER_SQL('SRC', 'IDS')).toBe(
      'SELECT native.* FROM (SRC) native WHERE native.order_id IN (IDS)',
    );
    expect(COMMERCE_SETTLEMENT_BY_ORDER_SQL('SRC', 'IDS', 'id')).toBe(
      'SELECT native.* FROM (SRC) native WHERE native.id IN (IDS)',
    );
    const s = salesCommerceSettlementSources;
    const e = ecommerceCommerceSettlementSources;
    expect(e.orders).toBe(COMMERCE_SETTLEMENT_BY_ORDER_SQL(s.orders, orderIds, 'id'));
    expect(e.lines).toBe(COMMERCE_SETTLEMENT_BY_ORDER_SQL(s.lines, orderIds));
    expect(e.deliveries).toBe(COMMERCE_SETTLEMENT_BY_ORDER_SQL(s.deliveries, orderIds));
    expect(e.deliveryLines).toBe(COMMERCE_SETTLEMENT_BY_ORDER_SQL(s.deliveryLines, orderIds));
    expect(e.returns).toBe(COMMERCE_SETTLEMENT_BY_ORDER_SQL(s.returns, orderIds));
  });

  it('renders byte-identical owner-local schema-ready SQL', () => {
    expect(COMMERCE_SCHEMA_READY_SQL('commerce_mfg_existing_posting_confirmations')).toBe(
      "SELECT to_regclass('commerce_mfg_existing_posting_confirmations') IS NOT NULL ready",
    );
    expect(COMMERCE_SCHEMA_READY_SQL('billing_test_finance_documents')).toBe(
      "SELECT to_regclass('billing_test_finance_documents') IS NOT NULL ready",
    );
  });

  it('keeps one literal per shared helper and leaves no inline duplicates or locks changed', async () => {
    const shared = await readFile(adapters('read-tail-r15-dedupe-sql.ts'), 'utf8');
    expect(shared.split('SELECT').length - 1).toBe(2);

    const sources = await readFile(adapters('ecommerce-package-sources.ts'), 'utf8');
    // The four remaining literals are the distinct return/goods/invoice families.
    expect(sources.split('SELECT native.* FROM (${').length - 1).toBe(4);
    expect(sources.split('native.order_id IN (${orderIds})').length - 1).toBe(0);
    expect(sources.split('native.id IN (${orderIds})').length - 1).toBe(0);
    expect(sources.split('COMMERCE_SETTLEMENT_BY_ORDER_SQL').length - 1).toBe(6); // import + five call sites
    expect(sources).not.toContain('LOCK TABLE');

    const mfg = await readFile(adapters('manufacturing-existing-posting-setup.ts'), 'utf8');
    expect(mfg.split("SELECT to_regclass('commerce_mfg_existing_posting_confirmations') IS NOT NULL ready").length - 1).toBe(0);
    expect(mfg.split("COMMERCE_SCHEMA_READY_SQL('commerce_mfg_existing_posting_confirmations')").length - 1).toBe(1);
    expect(mfg.split('LOCK TABLE commerce_mfg_existing_posting_confirmations IN SHARE ROW EXCLUSIVE MODE').length - 1).toBe(1);

    const billing = await readFile(adapters('billing-test-finance-ready.ts'), 'utf8');
    expect(billing.split("SELECT to_regclass('billing_test_finance_documents') IS NOT NULL ready").length - 1).toBe(0);
    expect(billing.split("COMMERCE_SCHEMA_READY_SQL('billing_test_finance_documents')").length - 1).toBe(1);
  });
});
