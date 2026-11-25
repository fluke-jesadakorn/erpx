import { beforeAll, afterAll, beforeEach, afterEach, describe, expect, it, vi } from 'vitest';
import { createHash } from 'node:crypto';
import { createServer, type IncomingMessage, type Server } from 'node:http';
import fs from 'node:fs';
import path from 'node:path';

vi.mock('server-only', () => ({}));

import {
  LANE14_COLUMNS,
  NATIVE_LANE14_CONFIG_ENV,
  nativeLane14Configured,
  nativeLane14Document,
  nativeLane14Page,
  nativeLane14ReferenceRows,
  type CommerceLane14Row,
} from '../../../packages/server-commerce/src/adapters/native-lane-14/client';
import {
  workflowDocument,
  workflowDocumentPage,
  workflowReferenceRows,
} from '../../../packages/server-commerce/src/adapters/workflow-read-sources';
import {
  NATIVE_LANE14_SALES_CONFIG_ENV,
  SALES_COUNT_KEYS,
  SALES_OPEN_ORDER_KEYS,
  SALES_PRODUCT_KEYS,
  SALES_PRODUCT_POINT_KEYS,
  SALES_SUPPORT_KEYS,
  nativeLane14SalesConfigured,
  nativeSalesCustomerOpenOrders,
  nativeSalesOrderCounts,
  nativeSalesPricingProduct,
  nativeSalesPricingProducts,
  nativeSalesSupportOrderSource,
} from '../../../packages/server-commerce/src/adapters/native-lane-14/sales-client';
import {
  salesCustomerOpenOrders,
  salesOrderCounts,
  salesPricingProduct,
  salesPricingProducts,
  salesSupportOrderSource,
} from '../../../packages/server-commerce/src/adapters/sales-order-sources';

const root = process.cwd();
const sha = (value: string) => createHash('sha256').update(value).digest('hex');
const row: CommerceLane14Row = Object.fromEntries(
  LANE14_COLUMNS.map((column, index) => [column, index % 2 === 0 ? `${column}-${index}` : index]),
);
const JSON_ROW_ORACLE = 'ce412ed75151fafe9e09c40c4582e98140f8ae6b931a9a826ce95e629619039e';
const JSON_COLUMNS_ORACLE = 'ed1ab44d556164169fcfd188db0dc59bc6e3bc905bd93d081d9d8f1936e975f2';
const RETIRED_SQL_ORACLES = [
  '9d28de781a98785f44379455b7062e8d7b9438634001c16ba91e049406df6d8b',
  '39d63f741e627b34d20c246af1fe682b4fe335af24cddf78853e9c9d7a688f7b',
  '24c6e6d4950d17a27762f0760f4501fb823e2b7da062c3d1f20f38d0232b505e',
];
const RETIRED_SQL_JOINED_ORACLE = '6c6ef3b5dfc52a7fd03ee86172186eb818dc0788a8bc8c8f1f951e93502f0d90';

type Seen = { url: string; host: string | undefined; origin: string | undefined; company: string | undefined; record: string | undefined };
let server: Server;
let requests: Seen[];
let port: number;
let mode: 'ok' | 'empty' | 'bad' | 'down' = 'ok';
let original: string | undefined;

const respond = (res: import('node:http').ServerResponse, body: unknown, status = 200) => {
  res.writeHead(status, { 'content-type': 'application/json' });
  res.end(JSON.stringify(body));
};

beforeAll(async () => {
  requests = [];
  server = createServer((req: IncomingMessage, res) => {
    const url = new URL(req.url ?? '/', 'http://127.0.0.1');
    requests.push({
      url: url.pathname,
      host: req.headers.host,
      origin: req.headers.origin,
      company: req.headers['x-erpx-context-company'] as string | undefined,
      record: req.headers['x-erpx-context-record'] as string | undefined,
    });
    if (mode === 'down') return respond(res, { error: { code: 'DOWN', message: 'down' } }, 503);
    if (mode === 'bad') return respond(res, { row: { id: 'only-id' } });
    if (url.pathname.endsWith('workflow-document'))
      return respond(res, { row: mode === 'empty' ? null : row });
    return respond(res, { rows: mode === 'empty' ? [] : [row] });
  });
  await new Promise<void>(resolve => server.listen(0, '127.0.0.1', resolve));
  const address = server.address();
  if (!address || typeof address === 'string') throw new Error('no server port');
  port = address.port;
});

afterAll(async () => {
  await new Promise<void>(resolve => server.close(() => resolve()));
});

beforeEach(() => {
  mode = 'ok';
  requests = [];
  original = process.env[NATIVE_LANE14_CONFIG_ENV];
  process.env[NATIVE_LANE14_CONFIG_ENV] = JSON.stringify({
    privateBaseUrl: `http://127.0.0.1:${port}`,
    host: 'acme.jesadakorn.com',
    holdingSlug: 'acme',
    origin: 'https://acme.jesadakorn.com',
    timeoutMs: 1000,
  });
});

afterEach(() => {
  if (original === undefined) delete process.env[NATIVE_LANE14_CONFIG_ENV];
  else process.env[NATIVE_LANE14_CONFIG_ENV] = original;
});

describe('native Commerce lane 14 workflow order reads', () => {
  it('pins the retired owner SQL and result projection oracles', () => {
    const retired = [
      'SELECT * FROM commerce_orders WHERE company_id=? AND id=?',
      'SELECT * FROM commerce_orders WHERE company_id=? AND id>? ORDER BY id LIMIT 100',
      'SELECT * FROM commerce_orders WHERE company_id=? ORDER BY updated_at DESC',
    ];
    expect(retired.map(sha)).toEqual(RETIRED_SQL_ORACLES);
    expect(sha(retired.join('\n'))).toBe(RETIRED_SQL_JOINED_ORACLE);
    expect(sha(JSON.stringify(row))).toBe(JSON_ROW_ORACLE);
    expect(sha(JSON.stringify(LANE14_COLUMNS))).toBe(JSON_COLUMNS_ORACLE);
    expect(LANE14_COLUMNS).toHaveLength(27);
  });

  it('delegates all three owner reads to the native routes with isolated headers', async () => {
    const db = {} as never;
    const document = await workflowDocument(db, 'order', 'co-1', 'order-1');
    const page = await workflowDocumentPage(db, 'order', 'co-1', 'cursor-1');
    const references = await workflowReferenceRows(db, 'order', 'co-1');
    expect(document).toEqual(row);
    expect(page.results).toEqual([row]);
    expect(references.results).toEqual([row]);
    expect(requests.map(request => request.url)).toEqual([
      '/api/v1/holdings/acme/commerce/lane14/workflow-document',
      '/api/v1/holdings/acme/commerce/lane14/workflow-document-page',
      '/api/v1/holdings/acme/commerce/lane14/workflow-reference-rows',
    ]);
    expect(requests.map(request => request.host)).toEqual([
      'acme.jesadakorn.com', 'acme.jesadakorn.com', 'acme.jesadakorn.com',
    ]);
    expect(requests.map(request => request.origin)).toEqual([
      'https://acme.jesadakorn.com', 'https://acme.jesadakorn.com', 'https://acme.jesadakorn.com',
    ]);
    expect(requests.map(request => request.company)).toEqual(['co-1', 'co-1', 'co-1']);
    expect(requests.map(request => request.record)).toEqual(['order-1', 'cursor-1', undefined]);
  });

  it('preserves a missing order as null and an empty page as an empty result', async () => {
    mode = 'empty';
    expect(await nativeLane14Document('co-1', 'absent')).toBeNull();
    expect((await nativeLane14Page('co-1', 'cursor')).results).toEqual([]);
    expect((await nativeLane14ReferenceRows('co-1')).results).toEqual([]);
  });

  it('fails closed when the configuration is absent or malformed', async () => {
    delete process.env[NATIVE_LANE14_CONFIG_ENV];
    expect(nativeLane14Configured()).toBe(false);
    await expect(nativeLane14Document('co-1', 'order-1')).rejects.toMatchObject({
      status: 503,
      code: 'COMMERCE_LANE14_READ_CONFIG_INVALID',
    });
    process.env[NATIVE_LANE14_CONFIG_ENV] = 'not-json';
    await expect(nativeLane14Page('co-1', 'cursor-1')).rejects.toMatchObject({ status: 503 });
    process.env[NATIVE_LANE14_CONFIG_ENV] = JSON.stringify({
      privateBaseUrl: `http://127.0.0.1:${port}`,
      host: 'wrong.example.com',
      holdingSlug: 'acme',
      origin: 'https://acme.jesadakorn.com',
      timeoutMs: 1000,
    });
    await expect(nativeLane14ReferenceRows('co-1')).rejects.toMatchObject({ status: 503 });
  });

  it('fails closed when the native service is down or the reply is malformed', async () => {
    mode = 'down';
    await expect(nativeLane14Document('co-1', 'order-1')).rejects.toMatchObject({ status: 503 });
    mode = 'bad';
    await expect(nativeLane14Page('co-1', 'cursor-1')).rejects.toMatchObject({
      status: 503,
      code: 'COMMERCE_LANE14_READ_RESPONSE_INVALID',
    });
    await expect(workflowDocument({} as never, 'order', 'co-1', 'order-1')).rejects.toMatchObject({
      status: 503,
    });
  });

  it('rejects unbounded company or record identities before issuing a request', async () => {
    await expect(nativeLane14Document('bad company!', 'order-1')).rejects.toMatchObject({ status: 503 });
    await expect(nativeLane14Page('co-1', '')).rejects.toMatchObject({ status: 503 });
    expect(requests).toHaveLength(0);
  });

  it('removed every owner statement from the TypeScript read source', () => {
    const source = fs.readFileSync(
      path.join(root, 'packages/server-commerce/src/adapters/workflow-read-sources.ts'),
      'utf8',
    );
    expect(source).not.toMatch(/SELECT /);
    expect(source).not.toMatch(/\.prepare\(/);
    expect(source).not.toMatch(/\.bind\(/);
    expect(source).toContain("from './native-lane-14/client'");
  });

  it('keeps the brief-listed owner SELECT-space count at 166 after the r27 renderer deletion', () => {
    const brief = fs.readFileSync(
      path.join(root, '.scratch/wave0/briefs/lane_14.md'), 'utf8',
    );
    const files = [...brief.matchAll(/^- `(packages\/server-commerce\/src\/adapters\/[^`]+\.ts)`$/gm)]
      .map(match => match[1]);
    let count = 0;
    for (const file of files) {
      const absolute = path.join(root, file);
      if (!fs.existsSync(absolute)) continue;
      count += fs.readFileSync(absolute, 'utf8').split('SELECT ').length - 1;
    }
    expect(count).toBe(166);
  });
});

describe('native Commerce lane 14 multi-holding Sales reads', () => {
  const productRow = { id: 'product-1', sku: 'SKU-1', name: 'Widget', tax_bps: 700 };
  const productPoint = { id: 'product-1', version: 3 };
  const countRow = { party_id: 'party-1', count: 2 };
  const openOrder = {
    id: 'order-1', number: 'SO-1', kind: 'sales', state: 'confirmed',
    due_date: '2026-12-01', currency: 'USD', total_minor: 12345, version: 2,
  };
  const supportRow = { id: 'order-2' };
  const sorted = (value: Record<string, unknown>) =>
    JSON.stringify(Object.fromEntries(Object.entries(value).sort(([a], [b]) => a.localeCompare(b))));

  const SQL_ORACLES = [
    '3c9de1470c375e7c3787511f34544b637a5440f92248fa8671e52da0ef7d177a',
    '34dbd55e3ba33962a21d9352dd2ff074807deb4bd4fe2dd5d481a0e794947460',
    '409d3c8f3b2b0a3705683ee546fbadc3fb9a63f3336d0c0dcb11cbc2640d19e5',
    '64de36fd517ec90e29e15b6a3ea0e7b8d1a60e4ae8dd9f107bb47f11fb2166f5',
    '909df75fecd0af7faa90b2ced8d44b5096f148236a3fe5cdd354265273a6ea29',
  ];
  const SQL_JOINED_ORACLE = '5c97d39db999d5b406cc990b067f435f550560597e6865f65fc9e1c7a44cec97';

  type SalesActor = Parameters<typeof nativeSalesPricingProducts>[0];
  const actor: SalesActor = {
    holdingId: 'h-acme', holdingSlug: 'acme', companyId: 'co-1',
    branchId: 'br-1', departmentId: null,
  } as SalesActor;
  const northstar = {
    holdingId: 'h-north', holdingSlug: 'northstar', companyId: 'co-1',
    branchId: null, departmentId: null,
  } as SalesActor;

  type SalesSeen = {
    url: string; method: string; host: string | undefined; origin: string | undefined;
    company: string | undefined; branch: string | undefined; department: string | undefined;
    record: string | undefined; body: unknown;
  };
  let salesServer: Server;
  let salesPort: number;
  let salesRequests: SalesSeen[];
  let salesMode: 'ok' | 'down' | 'bad' = 'ok';
  let savedSalesEnv: string | undefined;

  beforeAll(async () => {
    salesRequests = [];
    salesServer = createServer((req, res) => {
      const url = new URL(req.url ?? '/', 'http://127.0.0.1');
      const chunks: Buffer[] = [];
      req.on('data', chunk => chunks.push(Buffer.from(chunk)));
      req.on('end', () => {
        const text = Buffer.concat(chunks).toString('utf8');
        salesRequests.push({
          url: url.pathname,
          method: req.method ?? '',
          host: req.headers.host,
          origin: req.headers.origin,
          company: req.headers['x-erpx-context-company'] as string | undefined,
          branch: req.headers['x-erpx-context-branch'] as string | undefined,
          department: req.headers['x-erpx-context-department'] as string | undefined,
          record: req.headers['x-erpx-context-record'] as string | undefined,
          body: text ? JSON.parse(text) : undefined,
        });
        if (salesMode === 'down') return respond(res, { error: { code: 'DOWN', message: 'down' } }, 503);
        if (salesMode === 'bad') return respond(res, { rows: [{ id: 'only-id' }] });
        if (url.pathname.endsWith('sales-pricing-products')) return respond(res, { rows: [productRow] });
        if (url.pathname.endsWith('sales-pricing-product')) return respond(res, { row: productPoint });
        if (url.pathname.endsWith('sales-order-counts')) return respond(res, { rows: [countRow] });
        if (url.pathname.endsWith('sales-customer-open-orders')) return respond(res, { rows: [openOrder] });
        if (url.pathname.endsWith('sales-support-order-source')) return respond(res, { row: supportRow });
        return respond(res, { rows: [] });
      });
    });
    await new Promise<void>(resolve => salesServer.listen(0, '127.0.0.1', resolve));
    const address = salesServer.address();
    if (!address || typeof address === 'string') throw new Error('no Sales server port');
    salesPort = address.port;
  });

  afterAll(async () => {
    await new Promise<void>(resolve => salesServer.close(() => resolve()));
  });

  beforeEach(() => {
    salesMode = 'ok';
    salesRequests = [];
    savedSalesEnv = process.env[NATIVE_LANE14_SALES_CONFIG_ENV];
    process.env[NATIVE_LANE14_SALES_CONFIG_ENV] = JSON.stringify({
      privateBaseUrl: `http://127.0.0.1:${salesPort}`,
      timeoutMs: 1000,
      holdings: [
        {
          origin: 'https://acme.jesadakorn.com', host: 'acme.jesadakorn.com',
          holdingSlug: 'acme', holdingId: 'h-acme',
        },
        {
          origin: 'https://northstar.jesadakorn.com', host: 'northstar.jesadakorn.com',
          holdingSlug: 'northstar', holdingId: 'h-north',
        },
      ],
    });
  });

  afterEach(() => {
    if (savedSalesEnv === undefined) delete process.env[NATIVE_LANE14_SALES_CONFIG_ENV];
    else process.env[NATIVE_LANE14_SALES_CONFIG_ENV] = savedSalesEnv;
  });

  it('pins the retired Sales SQL and result projection oracles', () => {
    const retired = [
      "SELECT id,sku,name,tax_bps FROM commerce_products WHERE company_id=? AND status='active' ORDER BY sku",
      "SELECT id,version FROM commerce_products WHERE id=? AND company_id=? AND status='active'",
      "SELECT o.party_id,COUNT(*) count FROM commerce_orders o WHERE o.company_id=? AND (?::text IS NULL OR o.branch_id=?) AND (?::text IS NULL OR o.department_id=?) AND o.party_id IN (?,?) AND o.kind IN ('quote','sales') AND o.state NOT IN ('fulfilled','converted','cancelled') GROUP BY o.party_id",
      "SELECT o.id,o.number,o.kind,o.state,o.due_date,o.currency,(o.subtotal_minor+o.tax_minor) total_minor,o.version FROM commerce_orders o WHERE o.company_id=? AND (?::text IS NULL OR o.branch_id=?) AND (?::text IS NULL OR o.department_id=?) AND o.party_id=? AND o.kind IN ('quote','sales') AND o.state NOT IN ('fulfilled','converted','cancelled') ORDER BY o.due_date,o.id LIMIT 30",
      "SELECT o.id FROM commerce_orders o WHERE o.company_id=? AND (?::text IS NULL OR o.branch_id=?) AND (?::text IS NULL OR o.department_id=?) AND o.party_id=? AND o.kind IN ('quote','sales') ORDER BY o.updated_at DESC LIMIT 1",
    ];
    expect(retired.map(sha)).toEqual(SQL_ORACLES);
    expect(sha(retired.join('\n'))).toBe(SQL_JOINED_ORACLE);
    expect(sha(sorted(productRow))).toBe('091ea29c871fff5e9c70657ec69f0f648ac607a8183108cb10acfd47b4155f71');
    expect(sha(sorted(productPoint))).toBe('62771ef3ad2fd314e49f310d1f4ed5cefc6893c7bd9a5c6061505bcad2e4be1c');
    expect(sha(sorted(countRow))).toBe('376f5551224c63e500583160ac14d19bcfbdd6b7be4bdf95c6c62909ec8333c4');
    expect(sha(sorted(openOrder))).toBe('991e393828df803bf5f238a07f556251f185014a57d91a3dfb10aaa771431cb4');
    expect(sha(sorted(supportRow))).toBe('6c08f1ca016a4892425c4e530f700f28a968a35512484ac68c37c8c96dd04f4f');
    expect([SALES_PRODUCT_KEYS, SALES_PRODUCT_POINT_KEYS, SALES_COUNT_KEYS, SALES_OPEN_ORDER_KEYS, SALES_SUPPORT_KEYS].map(keys => keys.length))
      .toEqual([4, 2, 2, 8, 1]);
  });

  it('delegates all five owner reads with isolated multi-Holding headers', async () => {
    const db = {} as never;
    const fullActor = actor as never;
    expect((await salesPricingProducts(db, fullActor)).results).toEqual([productRow]);
    expect(await salesPricingProduct(db, fullActor, 'product-1')).toEqual(productPoint);
    expect(await salesOrderCounts(db, fullActor, ['party-1', 'party-2'])).toEqual([countRow]);
    expect(await salesCustomerOpenOrders(db, fullActor, 'party-1')).toEqual([openOrder]);
    expect(await salesSupportOrderSource(db, fullActor, 'party-1')).toEqual(supportRow);
    expect(salesRequests.map(item => item.url)).toEqual([
      '/api/v1/holdings/acme/commerce/lane14/sales-pricing-products',
      '/api/v1/holdings/acme/commerce/lane14/sales-pricing-product',
      '/api/v1/holdings/acme/commerce/lane14/sales-order-counts',
      '/api/v1/holdings/acme/commerce/lane14/sales-customer-open-orders',
      '/api/v1/holdings/acme/commerce/lane14/sales-support-order-source',
    ]);
    expect(salesRequests[0]).toMatchObject({
      method: 'GET', host: 'acme.jesadakorn.com', origin: 'https://acme.jesadakorn.com',
      company: 'co-1', branch: 'br-1', department: undefined,
    });
    expect(salesRequests[2]).toMatchObject({ method: 'POST', body: { partyIds: ['party-1', 'party-2'] } });
    expect(salesRequests[3].record).toBe('party-1');
  });

  it('selects the exact Holding and never falls back across pools', async () => {
    expect(nativeLane14SalesConfigured()).toBe(true);
    await nativeSalesPricingProducts(northstar);
    expect(salesRequests[0]).toMatchObject({
      url: '/api/v1/holdings/northstar/commerce/lane14/sales-pricing-products',
      host: 'northstar.jesadakorn.com',
      origin: 'https://northstar.jesadakorn.com',
      branch: undefined,
    });
    await expect(nativeSalesPricingProducts({ ...actor, holdingId: 'unknown' } as never))
      .rejects.toMatchObject({ status: 503, code: 'COMMERCE_LANE14_SALES_HOLDING_INVALID' });
    expect(salesRequests).toHaveLength(1);
  });

  it('fails closed on missing config, down service and malformed replies', async () => {
    delete process.env[NATIVE_LANE14_SALES_CONFIG_ENV];
    await expect(nativeSalesPricingProducts(actor)).rejects.toMatchObject({
      status: 503, code: 'COMMERCE_LANE14_SALES_CONFIG_INVALID',
    });
    process.env[NATIVE_LANE14_SALES_CONFIG_ENV] = JSON.stringify({
      privateBaseUrl: `http://127.0.0.1:${salesPort}`, timeoutMs: 1000,
      holdings: [{ origin: 'https://acme.jesadakorn.com', host: 'wrong.example.com', holdingSlug: 'acme', holdingId: 'h-acme' }],
    });
    await expect(nativeSalesPricingProducts(actor)).rejects.toMatchObject({ status: 503 });
    process.env[NATIVE_LANE14_SALES_CONFIG_ENV] = JSON.stringify({
      privateBaseUrl: `http://127.0.0.1:${salesPort}`, timeoutMs: 1000,
      holdings: [{ origin: 'https://acme.jesadakorn.com', host: 'acme.jesadakorn.com', holdingSlug: 'acme', holdingId: 'h-acme' }],
    });
    salesMode = 'down';
    await expect(nativeSalesOrderCounts(actor, ['party-1'])).rejects.toMatchObject({ status: 503 });
    salesMode = 'bad';
    await expect(nativeSalesPricingProducts(actor)).rejects.toMatchObject({
      status: 503, code: 'COMMERCE_LANE14_SALES_RESPONSE_INVALID',
    });
    await expect(salesPricingProducts({} as never, actor as never)).rejects.toMatchObject({ status: 503 });
  });

  it('removes the five replaced owner statements from the Sales source', () => {
    const source = fs.readFileSync(
      path.join(root, 'packages/server-commerce/src/adapters/sales-order-sources.ts'),
      'utf8',
    );
    expect(source.split('SELECT ').length - 1).toBe(3);
    expect(source).not.toMatch(/status='active' ORDER BY sku/);
    expect(source).not.toMatch(/COUNT\(\*\) count/);
    expect(source).not.toMatch(/total_minor,o\.version/);
    expect(source).toContain("from './native-lane-14/sales-client'");
  });
});
