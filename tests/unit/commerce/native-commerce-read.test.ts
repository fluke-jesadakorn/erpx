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
import { readFileSync } from 'node:fs';
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
import { COMMERCE_SETTLEMENT_BY_ORDER_SQL } from '../../../packages/server-commerce/src/adapters/read-tail-r15-dedupe-sql';
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
    expect(attachment.split('EXISTS(SELECT 1 FROM commerce_fulfillments f WHERE f.id=child.fulfillment_id AND f.order_id=o.id)').length - 1).toBe(0); // both fragments now rendered by fulfillmentParentGuardSql
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

  it('owns byte-identical schema-ready probes as native command-plan kinds', () => {
    const plans = readFileSync(adapters('native_commerce_read_sql.rs'), 'utf8');
    for (const [kind, wire, sql] of [
      ['RD_BILLING_TEST_FINANCE_SCHEMA', 'commerce_rd_billing_test_finance_schema', "SELECT to_regclass('billing_test_finance_documents') IS NOT NULL ready"],
      ['RD_MFG_EXISTING_POSTING_SCHEMA', 'commerce_rd_mfg_existing_posting_schema', "SELECT to_regclass('commerce_mfg_existing_posting_confirmations') IS NOT NULL ready"],
    ]) {
      expect(plans).toContain(`pub const ${kind}: &str = "${wire}";`);
      expect(plans).toContain(`"${sql}"`);
      expect(plans).toContain(`kind: ${kind},`);
      expect(readFileSync(adapters('native-commerce-read.ts'), 'utf8')).toContain(
        `export const ${kind} = '${wire}';`,
      );
    }
    expect(plans).toContain('assert_eq!(STATEMENTS.len(), 63);');
    expect(plans).toContain('fn schema_ready_probes_keep_their_exact_literals()');
    expect(readFileSync(adapters('read-tail-r15-dedupe-sql.ts'), 'utf8')).not.toContain('to_regclass');
  });

  it('keeps one literal per shared helper and leaves no inline duplicates or locks changed', async () => {
    const shared = await readFile(adapters('read-tail-r15-dedupe-sql.ts'), 'utf8');
    expect(shared.split('SELECT').length - 1).toBe(1);

    const sources = await readFile(adapters('ecommerce-package-sources.ts'), 'utf8');
    // The four remaining literals are the distinct return/goods/invoice families.
    expect(sources.split('SELECT native.* FROM (${').length - 1).toBe(4);
    expect(sources.split('native.order_id IN (${orderIds})').length - 1).toBe(0);
    expect(sources.split('native.id IN (${orderIds})').length - 1).toBe(0);
    expect(sources.split('COMMERCE_SETTLEMENT_BY_ORDER_SQL').length - 1).toBe(6); // import + five call sites
    expect(sources).not.toContain('LOCK TABLE');

    const mfg = await readFile(adapters('manufacturing-existing-posting-setup.ts'), 'utf8');
    expect(mfg).not.toContain('to_regclass');
    expect(mfg.split('nativeCommerceReadFirst<boolean>(RD_MFG_EXISTING_POSTING_SCHEMA, [], database)').length - 1).toBe(1);
    expect(mfg.split('LOCK TABLE commerce_mfg_existing_posting_confirmations IN SHARE ROW EXCLUSIVE MODE').length - 1).toBe(1);

    const billing = await readFile(adapters('billing-test-finance-ready.ts'), 'utf8');
    expect(billing).not.toContain('to_regclass');
    expect(billing.split("nativeCommerceReadFirst<{ready:boolean}>(RD_BILLING_TEST_FINANCE_SCHEMA,[],registeredDatabases(env,'h003').write)").length - 1).toBe(1);
  });

  it('owns the legacy attachment scope read as a native kind with no TypeScript SQL', async () => {
    const sql =
      'SELECT company_id,branch_id branch_id,department_id department_id,version FROM commerce_orders WHERE id=?';
    const plans = readFileSync(adapters('native_commerce_read_sql.rs'), 'utf8');
    expect(plans).toContain(
      'pub const RD_LEGACY_ATTACHMENT_SCOPE: &str = "commerce_rd_legacy_attachment_scope";',
    );
    expect(plans).toContain(`"${sql}"`);
    expect(plans).toContain('kind: RD_LEGACY_ATTACHMENT_SCOPE,');
    expect(plans).toContain('fn legacy_attachment_scope_keeps_its_exact_literal()');
    expect(plans).toContain('assert_eq!(STATEMENTS.len(), 63);');
    expect(readFileSync(adapters('native-commerce-read.ts'), 'utf8')).toContain(
      "export const RD_LEGACY_ATTACHMENT_SCOPE = 'commerce_rd_legacy_attachment_scope';",
    );
    const source = await readFile(adapters('attachment-migration-owner.ts'), 'utf8');
    expect(source).not.toContain('SELECT ');
    expect(source).not.toContain('descriptors');
    expect(source).toContain('"commerce_order_delivery":RD_LEGACY_ATTACHMENT_SCOPE');
    expect(source).toContain(
      'nativeCommerceReadFirst<{company_id:string;branch_id:string|null;department_id:string|null;version:number}>(ownerKind,[id],db)',
    );
    expect(source).toContain('if(!ownerKind)return Promise.resolve(null)');
  });

  it('owns the expense-posting prerequisite settings source as a native kind', async () => {
    const sql = 'SELECT to_jsonb(s)::text source FROM commerce_settings s WHERE s.company_id=?';
    const plans = readFileSync(adapters('native_commerce_read_sql.rs'), 'utf8');
    expect(plans).toContain('pub const RD_EXPENSE_PREREQUISITE_SETTINGS_SOURCE: &str =');
    expect(plans).toContain('"commerce_rd_expense_prerequisite_settings_source"');
    expect(plans).toContain(`"${sql}"`);
    expect(plans).toContain('kind: RD_EXPENSE_PREREQUISITE_SETTINGS_SOURCE,');
    expect(plans).toContain('fn expense_prerequisite_settings_source_keeps_its_exact_literal()');
    expect(readFileSync(adapters('native-commerce-read.ts'), 'utf8')).toContain(
      "export const RD_EXPENSE_PREREQUISITE_SETTINGS_SOURCE = 'commerce_rd_expense_prerequisite_settings_source';",
    );
    const source = await readFile(adapters('expense-posting-prerequisite-sources.ts'), 'utf8');
    expect(source).not.toContain('SELECT ');
    expect(source).toContain(
      'nativeCommerceReadFirst<{source:string}>(RD_EXPENSE_PREREQUISITE_SETTINGS_SOURCE,[actor.companyId],database)',
    );
  });

  it('owns the supplier-bill attachment receipt-id read as a native kind', async () => {
    const sql =
      "SELECT f.id FROM commerce_fulfillments f JOIN commerce_orders o ON o.id=f.order_id AND o.company_id=? AND o.kind='purchase' WHERE f.invoice_id=? ORDER BY f.id";
    const plans = readFileSync(adapters('native_commerce_read_sql.rs'), 'utf8');
    expect(plans).toContain(
      'pub const RD_SUPPLIER_BILL_RECEIPT_IDS: &str = "commerce_rd_supplier_bill_receipt_ids";',
    );
    expect(plans).toContain(`"${sql}"`);
    expect(plans).toContain('kind: RD_SUPPLIER_BILL_RECEIPT_IDS,');
    expect(plans).toContain('fn supplier_bill_receipt_ids_keep_their_exact_literal()');
    expect(readFileSync(adapters('native-commerce-read.ts'), 'utf8')).toContain(
      "export const RD_SUPPLIER_BILL_RECEIPT_IDS = 'commerce_rd_supplier_bill_receipt_ids';",
    );
    const source = await readFile(adapters('supplier-bill-attachment-sources.ts'), 'utf8');
    expect(source).not.toContain('SELECT ');
    expect(source).toContain(
      'nativeCommerceReadAll<{id:string}>(RD_SUPPLIER_BILL_RECEIPT_IDS,[companyId,invoiceId],database)',
    );
    expect(source).toContain(
      "commerceAttachmentReadSourceImage(database,companyId,'commerce_fulfillment',receipt.id)",
    );
  });

  it('owns both POS private-image product reads as native kinds', async () => {
    const plans = readFileSync(adapters('native_commerce_read_sql.rs'), 'utf8');
    for (const [kind, wire, sql] of [
      ['RD_POS_PRODUCT_VERSION', 'commerce_rd_pos_product_version', 'SELECT version FROM commerce_products WHERE id=? AND company_id=?'],
      ['RD_POS_PRODUCT_IMAGE', 'commerce_rd_pos_product_image', 'SELECT to_jsonb(product)::text image FROM commerce_products product WHERE product.id=? AND product.company_id=?'],
    ]) {
      expect(plans).toContain(`pub const ${kind}: &str = "${wire}";`);
      expect(plans).toContain(`"${sql}"`);
      expect(plans).toContain(`kind: ${kind},`);
      expect(readFileSync(adapters('native-commerce-read.ts'), 'utf8')).toContain(
        `export const ${kind} = '${wire}';`,
      );
    }
    expect(plans).toContain('fn pos_image_product_reads_keep_their_exact_literals()');
    const source = await readFile(adapters('pos-images-facts.ts'), 'utf8');
    expect(source).not.toContain('SELECT ');
    expect(source).toContain(
      'nativeCommerceReadFirst<{version:number}>(RD_POS_PRODUCT_VERSION,[facts.productId,facts.companyId],db)',
    );
    expect(source).toContain(
      'nativeCommerceReadFirst<string>(RD_POS_PRODUCT_IMAGE,[productId,companyId],db)',
    );
    expect(source).toContain('LOCK TABLE commerce_products IN SHARE ROW EXCLUSIVE MODE');
  });

  it('owns both landed-cost reads as native kinds with no inline SQL', async () => {
    const plans = readFileSync(adapters('native_commerce_read_sql.rs'), 'utf8');
    for (const [kind, wire, sql] of [
      ['RD_LANDED_COST_RECEIPT_LINES', 'commerce_rd_landed_cost_receipt_lines', 'SELECT fl.*,ol.product_id,p.tracking_mode FROM commerce_fulfillment_lines fl JOIN commerce_order_lines ol ON ol.id=fl.order_line_id JOIN commerce_products p ON p.id=ol.product_id WHERE fl.fulfillment_id=? ORDER BY ol.ordinal'],
      ['RD_LANDED_COST_POSTING_CONFIG', 'commerce_rd_landed_cost_posting_config', 'SELECT chart_id,inventory_account_id FROM commerce_settings WHERE company_id=?'],
    ]) {
      expect(plans).toContain(`pub const ${kind}: &str = "${wire}";`);
      expect(plans).toContain(`"${sql}"`);
      expect(plans).toContain(`kind: ${kind},`);
      expect(readFileSync(adapters('native-commerce-read.ts'), 'utf8')).toContain(
        `export const ${kind} = '${wire}';`,
      );
    }
    expect(plans).toContain('fn landed_cost_reads_keep_their_exact_literals()');
    const source = await readFile(adapters('inventory-landed-costs-sources.ts'), 'utf8');
    expect(source).not.toContain('SELECT ');
    expect(source).toContain(
      'nativeCommerceReadAll<LandedCostRow>(RD_LANDED_COST_RECEIPT_LINES,[fulfillmentId],db)',
    );
    expect(source).toContain(
      'nativeCommerceReadFirst<LandedCostRow>(RD_LANDED_COST_POSTING_CONFIG,[companyId],db)',
    );
  });

  it('owns the transit EXISTS guards as native command-plan kinds', async () => {
    const plans = readFileSync(adapters('native_commerce_read_sql.rs'), 'utf8');
    for (const [kind, wire, sql] of [
      ['RD_TRANSIT_ACTIVE_LOCATION', 'commerce_rd_transit_active_location', "EXISTS(SELECT 1 FROM commerce_warehouses WHERE id=? AND company_id=? AND branch_id=? AND status='active')"],
      ['RD_TRANSIT_PRODUCT_VERSION', 'commerce_rd_transit_product_version', "EXISTS(SELECT 1 FROM commerce_products WHERE id=? AND company_id=? AND status='active' AND version=?)"],
    ]) {
      expect(plans).toContain(`pub const ${kind}: &str = "${wire}";`);
      expect(plans).toContain(`"${sql}"`);
      expect(plans).toContain(`kind: ${kind},`);
      expect(readFileSync(adapters('native-commerce-read.ts'), 'utf8')).toContain(
        `export const ${kind} = '${wire}';`,
      );
    }
    expect(plans).toContain('fn transit_guard_predicates_keep_their_exact_literals()');
    expect(readFileSync(adapters('native-commerce-read.ts'), 'utf8')).toContain(
      'export async function nativeCommerceReadPlan(',
    );
    const source = await readFile(adapters('inventory-transit-sources.ts'), 'utf8');
    expect(source).not.toContain('EXISTS(SELECT');
    expect(source).not.toContain('SELECT ');
    expect(source).toContain('ownerGuard(db,key,value,RD_TRANSIT_ACTIVE_LOCATION,[value2,company_id,value3])');
    expect(source).toContain('ownerGuard(db,key,value,RD_TRANSIT_PRODUCT_VERSION,[product_id,companyId,product_version])');
    expect(source).toContain('const planned=await nativeCommerceReadPlan(kind,args);return assertion(db,key,value,planned.sql,args);');
  });

  it('owns the POS full active-warehouses read as a native kind', async () => {
    const sql =
      "SELECT * FROM commerce_warehouses WHERE company_id=? AND status='active' AND (?::text IS NULL OR branch_id=?) ORDER BY name";
    const plans = readFileSync(adapters('native_commerce_read_sql.rs'), 'utf8');
    expect(plans).toContain('pub const RD_POS_WAREHOUSES_ALL: &str = "commerce_rd_pos_warehouses_all";');
    expect(plans).toContain(`"${sql}"`);
    expect(plans).toContain('kind: RD_POS_WAREHOUSES_ALL,');
    expect(plans).toContain('fn pos_warehouses_all_keeps_its_exact_literal()');
    expect(readFileSync(adapters('native-commerce-read.ts'), 'utf8')).toContain(
      "export const RD_POS_WAREHOUSES_ALL = 'commerce_rd_pos_warehouses_all';",
    );
    const source = await readFile(adapters('pos-facts.ts'), 'utf8');
    expect(source).not.toContain(sql);
    expect(source).toContain(
      'nativeCommerceReadAll<SqlValueRow>(RD_POS_WAREHOUSES_ALL,[facts.company,facts.branch,facts.branch2],db)',
    );
    expect(source).toContain('COUNT(DISTINCT p.id)::int count');
  });

  it('owns the existing-posting confirmation image read as a native kind', async () => {
    const sql =
      'SELECT to_jsonb(s)::text image FROM commerce_mfg_existing_posting_confirmations s WHERE company_id=?';
    const plans = readFileSync(adapters('native_commerce_read_sql.rs'), 'utf8');
    expect(plans).toContain(
      'pub const RD_MFG_EXISTING_POSTING_CONFIRMATION_IMAGE: &str =',
    );
    expect(plans).toContain('"commerce_rd_mfg_existing_posting_confirmation_image"');
    expect(plans).toContain(`"${sql}"`);
    expect(plans).toContain('kind: RD_MFG_EXISTING_POSTING_CONFIRMATION_IMAGE,');
    expect(plans).toContain('fn mfg_posting_confirmation_image_keeps_its_exact_literal()');
    expect(readFileSync(adapters('native-commerce-read.ts'), 'utf8')).toContain(
      "export const RD_MFG_EXISTING_POSTING_CONFIRMATION_IMAGE = 'commerce_rd_mfg_existing_posting_confirmation_image';",
    );
    const source = await readFile(adapters('manufacturing-existing-posting-setup.ts'), 'utf8');
    expect(source).not.toContain(sql);
    expect(source).toContain(
      'nativeCommerceReadFirst<string>(RD_MFG_EXISTING_POSTING_CONFIRMATION_IMAGE,[companyId],database)',
    );
    expect(source).toContain('SELECT to_jsonb(r)::text image FROM command_receipts r');
  });

  it('owns the sales delivery-notification line read as a native kind', async () => {
    const sql =
      'SELECT p.sku,fl.quantity_milli,p.unit FROM commerce_fulfillment_lines fl JOIN commerce_order_lines ol ON ol.id=fl.order_line_id JOIN commerce_products p ON p.id=ol.product_id WHERE fl.fulfillment_id=? ORDER BY ol.ordinal';
    const plans = readFileSync(adapters('native_commerce_read_sql.rs'), 'utf8');
    expect(plans).toContain('pub const RD_SALES_NOTIFICATION_LINES: &str = "commerce_rd_sales_notification_lines";');
    expect(plans).toContain(`"${sql}"`);
    expect(plans).toContain('kind: RD_SALES_NOTIFICATION_LINES,');
    expect(plans).toContain('fn sales_notification_lines_keep_their_exact_literal()');
    expect(readFileSync(adapters('native-commerce-read.ts'), 'utf8')).toContain(
      "export const RD_SALES_NOTIFICATION_LINES = 'commerce_rd_sales_notification_lines';",
    );
    const source = await readFile(adapters('sales-order-sources.ts'), 'utf8');
    expect(source).not.toContain(sql);
    expect(source).toContain(
      'nativeCommerceReadAll<Row>(RD_SALES_NOTIFICATION_LINES,[fulfillmentId],write)',
    );
    expect(source).toContain('financeProjectionSources.finance_parties');
    expect(source).toContain('salesProjectionSources.commerce_customer_notification_settings');
  });

  it('owns the purchase-control source and return-line reads as native kinds', async () => {
    const plans = readFileSync(adapters('native_commerce_read_sql.rs'), 'utf8');
    for (const [kind, wire, sql, binds] of [
      ['RD_PURCHASE_CONTROL_ORDER', 'commerce_rd_purchase_control_order', 'SELECT DISTINCT o.id,o.kind,o.company_id,o.branch_id,o.department_id FROM commerce_orders o WHERE o.id=?', 1],
      ['RD_PURCHASE_CONTROL_INVOICE', 'commerce_rd_purchase_control_invoice', 'SELECT DISTINCT o.id,o.kind,o.company_id,o.branch_id,o.department_id FROM commerce_orders o JOIN commerce_fulfillments f ON f.order_id=o.id WHERE f.invoice_id=?', 1],
      ['RD_PURCHASE_CONTROL_RETURN_LINES', 'commerce_rd_purchase_control_return_lines', 'SELECT r.id,EXISTS(SELECT 1 FROM commerce_return_lines rl JOIN commerce_fulfillment_lines fl ON fl.id=rl.fulfillment_line_id JOIN commerce_order_lines ol ON ol.id=fl.order_line_id WHERE rl.return_id=r.id AND ol.item_type!=\'service\') has_stock FROM commerce_returns r JOIN commerce_orders o ON o.id=r.order_id WHERE o.id=? AND o.company_id=? AND (? IS NULL OR o.branch_id=?) AND (? IS NULL OR o.department_id=?)', 6],
    ] as const) {
      expect(plans).toContain(`pub const ${kind}: &str = "${wire}";`);
      expect(plans).toContain(`"${sql}"`);
      expect(plans).toContain(`kind: ${kind},`);
      expect(plans).toContain(`${binds}usize`);
      expect(readFileSync(adapters('native-commerce-read.ts'), 'utf8')).toContain(
        `export const ${kind} = '${wire}';`,
      );
    }
    expect(plans).toContain('fn purchase_control_reads_keep_their_exact_literals()');
    const source = await readFile(adapters('purchase-controls.ts'), 'utf8');
    expect(source).not.toContain('SELECT ');
    expect(source).toContain('target.type === "invoice" ? RD_PURCHASE_CONTROL_INVOICE : RD_PURCHASE_CONTROL_ORDER');
    expect(source).toContain('nativeCommerceReadAll<{id:string;has_stock:boolean|number}>(RD_PURCHASE_CONTROL_RETURN_LINES,[orderId,actor.companyId,actor.branchId,actor.branchId,actor.departmentId,actor.departmentId],database)');
  });

  it('owns the initial-count warehouse and stock-product master reads', async () => {
    const plans = readFileSync(adapters('native_commerce_read_sql.rs'), 'utf8');
    for (const [kind, wire, sql] of [
      ['RD_INITIAL_COUNT_WAREHOUSES', 'commerce_rd_initial_count_warehouses', "SELECT id,name,branch_id FROM commerce_warehouses WHERE company_id=? AND status='active' AND (?::text IS NULL OR branch_id=?) ORDER BY name,id"],
      ['RD_INITIAL_COUNT_STOCK_PRODUCT', 'commerce_rd_initial_count_stock_product', "SELECT * FROM commerce_products WHERE id=? AND company_id=? AND status='active' AND item_type='stock'"],
    ] as const) {
      expect(plans).toContain(`pub const ${kind}: &str = "${wire}";`);
      expect(plans).toContain(`"${sql}"`);
      expect(plans).toContain(`kind: ${kind},`);
      expect(readFileSync(adapters('native-commerce-read.ts'), 'utf8')).toContain(
        `export const ${kind} = '${wire}';`,
      );
    }
    expect(plans).toContain('fn initial_count_master_reads_keep_their_exact_literals()');
    const source = await readFile(adapters('initial-physical-stock-count-sources.ts'), 'utf8');
    expect(source).not.toContain('SELECT id,name,branch_id FROM commerce_warehouses');
    expect(source).not.toContain("SELECT * FROM commerce_products WHERE id=? AND company_id=? AND status='active' AND item_type='stock'");
    expect(source).toContain('nativeCommerceReadAll<CountRow>(RD_INITIAL_COUNT_WAREHOUSES,[actor.companyId,actor.branchId,actor.branchId],database)');
    expect(source).toContain('nativeCommerceReadFirst<CountRow>(RD_INITIAL_COUNT_STOCK_PRODUCT,[productId,actor.companyId],database)');
    expect(source).toContain('return {warehouses,products};');
    expect(source).toContain('command_receipts');
  });

  it('owns the three PMS owner-local reads while the FOR UPDATE stock read stays inline', async () => {
    const plans = readFileSync(adapters('native_commerce_read_sql.rs'), 'utf8');
    for (const [kind, wire, sql] of [
      ['RD_PMS_PRODUCT_OPTIONS', 'commerce_rd_pms_product_options', "SELECT id,name FROM commerce_products WHERE company_id=? AND status='active'"],
      ['RD_PMS_WAREHOUSE_ID', 'commerce_rd_pms_warehouse_id', "SELECT id FROM commerce_warehouses WHERE id=? AND company_id=? AND branch_id=? AND status='active'"],
      ['RD_PMS_PRODUCT_WAREHOUSE_SCOPE', 'commerce_rd_pms_product_warehouse_scope', "SELECT 1 FROM commerce_products p JOIN commerce_warehouses w ON w.id=? WHERE p.id=? AND p.company_id=? AND w.company_id=p.company_id AND w.branch_id=? AND p.status='active' AND w.status='active'"],
    ] as const) {
      expect(plans).toContain(`pub const ${kind}: &str = "${wire}";`);
      expect(plans).toContain(`"${sql}"`);
      expect(plans).toContain(`kind: ${kind},`);
      expect(readFileSync(adapters('native-commerce-read.ts'), 'utf8')).toContain(
        `export const ${kind} = '${wire}';`,
      );
    }
    expect(plans).toContain('fn pms_owner_local_reads_keep_their_exact_literals()');
    const source = await readFile(adapters('pms-command-facts.ts'), 'utf8');
    for (const sql of [
      "SELECT id,name FROM commerce_products WHERE company_id=? AND status='active'",
      "SELECT id FROM commerce_warehouses WHERE id=? AND company_id=? AND branch_id=? AND status='active'",
      "SELECT 1 FROM commerce_products p JOIN commerce_warehouses w ON w.id=? WHERE p.id=? AND p.company_id=? AND w.company_id=p.company_id AND w.branch_id=? AND p.status='active' AND w.status='active'",
    ]) expect(source).not.toContain(sql);
    expect(source).toContain('nativeCommerceReadAll<{\n            id: string;\n            name: string;\n        }>(RD_PMS_PRODUCT_OPTIONS,[facts.companyId],db)');
    expect(source).toContain('nativeCommerceReadFirst<{id:string}>(RD_PMS_WAREHOUSE_ID,[facts.warehouseId,facts.companyId,facts.branchId],db)');
    expect(source).toContain("nativeCommerceReadFirst<{ '1': number }>(RD_PMS_PRODUCT_WAREHOUSE_SCOPE,[facts.warehouseId,facts.productId,facts.companyId,facts.branchId],db)");
    expect(source).toContain('FOR UPDATE');
  });

  it('owns the attachment product source image as a native kind', async () => {
    const sql = 'SELECT to_jsonb(p)::text image FROM commerce_products p WHERE p.id=? AND p.company_id=?';
    const plans = readFileSync(adapters('native_commerce_read_sql.rs'), 'utf8');
    expect(plans).toContain('pub const RD_ATTACHMENT_PRODUCT_IMAGE: &str = "commerce_rd_attachment_product_image";');
    expect(plans).toContain(`"${sql}"`);
    expect(plans).toContain('kind: RD_ATTACHMENT_PRODUCT_IMAGE,');
    expect(plans).toContain('fn attachment_product_image_keeps_its_exact_literal()');
    expect(readFileSync(adapters('native-commerce-read.ts'), 'utf8')).toContain(
      "export const RD_ATTACHMENT_PRODUCT_IMAGE = 'commerce_rd_attachment_product_image';",
    );
    const source = await readFile(adapters('attachment-read-sources.ts'), 'utf8');
    expect(source).not.toContain(sql);
    expect(source).toContain('nativeCommerceReadFirst<string>(RD_ATTACHMENT_PRODUCT_IMAGE,[id,companyId],database)');
    expect(source).toContain('commerceAttachmentPosSourceImage(database,companyId,id)');
  });

  it('owns all executive order-evidence variants as four native metric plans', async () => {
    const plans = readFileSync(adapters('native_commerce_read_sql.rs'), 'utf8');
    for (const [kind, wire, marker, binds] of [
      ['RD_EXEC_SALES_ORDERS_EVIDENCE', 'commerce_rd_exec_sales_orders_evidence', "o.state IN ('confirmed','partially_fulfilled','fulfilled') AND o.order_date BETWEEN ? AND ? AND o.currency=?", 16],
      ['RD_EXEC_SALES_CANCELLATIONS_EVIDENCE', 'commerce_rd_exec_sales_cancellations_evidence', "o.state='cancelled' AND o.updated_at>=? AND o.updated_at<?", 15],
      ['RD_EXEC_SALES_CANCELLATION_VALUE_EVIDENCE', 'commerce_rd_exec_sales_cancellation_value_evidence', 'o.approved_by IS NOT NULL AND o.currency=?', 16],
      ['RD_EXEC_SALES_FULFILLMENT_EVIDENCE', 'commerce_rd_exec_sales_fulfillment_evidence', 'EXISTS(SELECT 1 FROM commerce_order_lines l WHERE l.order_id=o.id)', 15],
    ] as const) {
      expect(plans).toContain(`pub const ${kind}: &str = "${wire}";`);
      expect(plans).toContain(marker);
      expect(plans).toContain(`kind: ${kind},`);
      expect(plans).toContain(`${binds}usize`);
      expect(readFileSync(adapters('native-commerce-read.ts'), 'utf8')).toContain(
        `export const ${kind} = '${wire}';`,
      );
    }
    expect(plans).toContain('fn executive_evidence_plans_cover_the_closed_metric_variants()');
    const source = await readFile(adapters('executive-main-evidence-sources.ts'), 'utf8');
    expect(source).not.toContain('SELECT ');
    expect(source).not.toContain('db.prepare(');
    expect(source).toContain('nativeCommerceReadAll<SalesOrderEvidenceRow>(orderEvidencePlan(metric)');
    expect(source).toContain('nativeCommerceReadAll<SalesFulfillmentEvidenceRow>(RD_EXEC_SALES_FULFILLMENT_EVIDENCE');
    expect(source).toContain('EVIDENCE_CURSOR_INVALID');
    expect(source).toContain('factsSearchSalesOrderEvidence');
    expect(source).toContain('factsReadSalesFulfillmentEvidence');
  });

  it('owns the inline executive projection fragments as native source authority', async () => {
    const plans = readFileSync(adapters('native_commerce_read_sql.rs'), 'utf8');
    for (const [kind, wire, marker] of [
      ['RD_PROJECTION_ORDER_LINES', 'commerce_rd_projection_order_lines', '\\"discount_bps\\",\\"item_type\\" FROM commerce_order_lines'],
      ['RD_PROJECTION_PRODUCTS', 'commerce_rd_projection_products', '\\"tracking_mode\\",\\"item_type\\" FROM commerce_products'],
      ['RD_PROJECTION_RETURN_LINES', 'commerce_rd_projection_return_lines', '\\"expected_subtotal_minor\\" FROM commerce_return_lines'],
      ['RD_PROJECTION_RETURN_LOT_ALLOCATIONS', 'commerce_rd_projection_return_lot_allocations', '\\"snapshot_version\\" FROM commerce_return_lot_allocations'],
    ] as const) {
      expect(plans).toContain(`pub const ${kind}: &str =`);
      expect(plans).toContain(`"${wire}"`);
      expect(plans).toContain(marker);
      expect(plans).toContain(`kind: ${kind},`);
      expect(readFileSync(adapters('native-commerce-read.ts'), 'utf8')).toContain(
        `export const ${kind} = '${wire}';`,
      );
    }
    expect(plans).toContain('fn executive_projection_fragments_keep_their_exact_literals()');
    expect(plans).toContain('assert_eq!(SOURCE_FRAGMENTS.len(), 6);');
    // Deletion is blocked: the cross-owner consumers embed these strings.
    const source = await readFile(adapters('inventory-executive-owner-projections.ts'), 'utf8');
    expect(source).toContain('commerce_order_lines: "SELECT');
    expect(source).toContain('COMMERCE_FULFILLMENT_LINES_SQL');
  });

  it('owns the three attachment policy descriptor reads as native kinds', async () => {
    const plans = readFileSync(adapters('native_commerce_read_sql.rs'), 'utf8');
    for (const [kind, wire, marker] of [
      ['RD_POLICY_PURCHASE_RECEIPT', 'commerce_rd_policy_purchase_receipt', "o.kind='purchase'"],
      ['RD_POLICY_FULFILLMENT', 'commerce_rd_policy_fulfillment', 'NULL::text department_id,o.kind,f.version'],
      ['RD_POLICY_PRODUCT', 'commerce_rd_policy_product', 'NULL::text branch_id,NULL::text department_id,version'],
    ] as const) {
      expect(plans).toContain(`pub const ${kind}: &str = "${wire}";`);
      expect(plans).toContain(marker);
      expect(plans).toContain(`kind: ${kind},`);
      expect(readFileSync(adapters('native-commerce-read.ts'), 'utf8')).toContain(
        `export const ${kind} = '${wire}';`,
      );
    }
    expect(plans).toContain('fn policy_descriptor_reads_keep_their_exact_literals()');
    const source = await readFile(adapters('attachments-policy-facts.ts'), 'utf8');
    for (const sql of [
      "SELECT o.company_id,o.branch_id,o.department_id,o.version FROM commerce_orders o WHERE o.id=? AND o.company_id=? AND o.kind='purchase'",
      'SELECT o.company_id,o.branch_id,NULL::text department_id,o.kind,f.version FROM commerce_fulfillments f JOIN commerce_orders o ON o.id=f.order_id WHERE f.id=? AND o.company_id=?',
      'SELECT company_id,NULL::text branch_id,NULL::text department_id,version FROM commerce_products WHERE id=? AND company_id=?',
    ]) expect(source).not.toContain(sql);
    expect(source).toContain('nativeCommerceReadFirst<{ company_id: string; branch_id: string | null; department_id: string | null; version: number; }>(RD_POLICY_PURCHASE_RECEIPT');
    expect(source).toContain('(RD_POLICY_FULFILLMENT,[facts.targetId,facts.companyId],db)');
    expect(source).toContain('(RD_POLICY_PRODUCT,[facts.targetId,facts.companyId],db)');
    expect(source).toContain('commerceOrdersReadSql');
  });

  it('owns the Commerce posting-prerequisite state witness as a native kind', async () => {
    const plans = readFileSync(adapters('native_commerce_read_sql.rs'), 'utf8');
    expect(plans).toContain('pub const RD_POSTING_PREREQUISITE_STATE: &str = "commerce_rd_posting_prerequisite_state";');
    expect(plans).toContain('kind: RD_POSTING_PREREQUISITE_STATE,');
    expect(plans).toContain('5usize');
    expect(plans).toContain('fn posting_prerequisite_state_keeps_its_exact_literal()');
    expect(readFileSync(adapters('native-commerce-read.ts'), 'utf8')).toContain(
      "export const RD_POSTING_PREREQUISITE_STATE = 'commerce_rd_posting_prerequisite_state';",
    );
    const source = await readFile(adapters('posting-prerequisite-sources.ts'), 'utf8');
    expect(source).not.toContain('SELECT ');
    expect(source).toContain('nativeCommerceReadFirst<{configured:boolean;history:boolean}>(RD_POSTING_PREREQUISITE_STATE,[companyId,companyId,companyId,companyId,companyId],database)');
    expect(source).toContain('Commerce initial posting state is unavailable.');
  });

  it('owns all inventory attachment image reads as native kinds', async () => {
    const plans = readFileSync(adapters('native_commerce_read_sql.rs'), 'utf8');
    for (const [kind, wire, marker, binds] of [
      ['RD_INVENTORY_ATTACHMENT_WAREHOUSES', 'commerce_rd_inventory_attachment_warehouses', 'jsonb_agg(to_jsonb(w) ORDER BY w.id)::text image', 2],
      ['RD_INVENTORY_ATTACHMENT_PRODUCTS', 'commerce_rd_inventory_attachment_products', 'jsonb_agg(to_jsonb(p) ORDER BY p.id)::text image', 2],
      ['RD_INVENTORY_ATTACHMENT_STOCK', 'commerce_rd_inventory_attachment_stock', 'jsonb_agg(to_jsonb(s) ORDER BY s.warehouse_id,s.product_id)::text image', 3],
      ['RD_INVENTORY_ATTACHMENT_RECEIPT', 'commerce_rd_inventory_attachment_receipt', "jsonb_build_object('receipt',to_jsonb(f)", 2],
    ] as const) {
      expect(plans).toContain(`pub const ${kind}: &str = "${wire}";`);
      expect(plans).toContain(marker);
      expect(plans).toContain(`kind: ${kind},`);
      expect(plans).toContain(`${binds}usize`);
      expect(readFileSync(adapters('native-commerce-read.ts'), 'utf8')).toContain(
        `export const ${kind} = '${wire}';`,
      );
    }
    expect(plans).toContain('fn inventory_attachment_images_keep_their_exact_literals()');
    const source = await readFile(adapters('inventory-attachment-sources.ts'), 'utf8');
    expect(source).not.toContain('SELECT ');
    expect(source).toContain('(RD_INVENTORY_ATTACHMENT_WAREHOUSES,[input.companyId,JSON.stringify(input.warehouseIds)],database)');
    expect(source).toContain('(RD_INVENTORY_ATTACHMENT_PRODUCTS,[input.companyId,JSON.stringify(input.productIds)],database)');
    expect(source).toContain('(RD_INVENTORY_ATTACHMENT_STOCK,[input.companyId,JSON.stringify(input.warehouseIds),JSON.stringify(input.productIds)],database)');
    expect(source).toContain('(RD_INVENTORY_ATTACHMENT_RECEIPT,[input.companyId,input.receiptId],database)');
    expect(source).toContain('LOCK TABLE commerce_warehouses,commerce_products,commerce_stock');
  });

  it('owns the workspace order-existence probe as a native kind', async () => {
    const plans = readFileSync(adapters('native_commerce_read_sql.rs'), 'utf8');
    expect(plans).toContain('pub const RD_WORKSPACE_ORDER_EXISTS: &str = "commerce_rd_workspace_order_exists";');
    expect(plans).toContain('"SELECT 1 FROM commerce_orders WHERE id=?"');
    expect(plans).toContain('kind: RD_WORKSPACE_ORDER_EXISTS,');
    expect(plans).toContain('fn workspace_order_exists_keeps_its_exact_literal()');
    expect(readFileSync(adapters('native-commerce-read.ts'), 'utf8')).toContain(
      "export const RD_WORKSPACE_ORDER_EXISTS = 'commerce_rd_workspace_order_exists';",
    );
    const source = await readFile(adapters('record-purchases.ts'), 'utf8');
    expect(source).not.toContain('SELECT 1 FROM commerce_orders WHERE id=?');
    expect(source).toContain('nativeCommerceReadFirst<{ \'1\': number }>(RD_WORKSPACE_ORDER_EXISTS,[id],database)');
    // Cross-owner predicate builders stay; they are not executable reads here.
    expect(source).toContain('assignmentReadSql');
    expect(source).toContain('purchasedCapabilityPredicate');
  });

  it('owns the order source-id lookup as a native kind', async () => {
    const plans = readFileSync(adapters('native_commerce_read_sql.rs'), 'utf8');
    expect(plans).toContain('pub const RD_ORDER_BY_SOURCE_ID: &str = "commerce_rd_order_by_source_id";');
    expect(plans).toContain('"SELECT id,number FROM commerce_orders WHERE source_id=?"');
    expect(plans).toContain('kind: RD_ORDER_BY_SOURCE_ID,');
    expect(plans).toContain('fn order_by_source_id_keeps_its_exact_literal()');
    expect(readFileSync(adapters('native-commerce-read.ts'), 'utf8')).toContain(
      "export const RD_ORDER_BY_SOURCE_ID = 'commerce_rd_order_by_source_id';",
    );
    const source = await readFile(adapters('commerce-order-read.ts'), 'utf8');
    expect(source).not.toContain('SELECT id,number FROM commerce_orders WHERE source_id=?');
    expect(source).toContain('nativeCommerceReadAll<Row>(RD_ORDER_BY_SOURCE_ID,[facts.id],db)');
    expect(source).toContain('people_payrollProjectionSources');
  });
});
