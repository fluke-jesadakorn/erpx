import 'server-only';
import { Agent, request } from 'node:http';
import { Readable } from 'node:stream';

/** Rendering input from retained page authorization, never a grant. */
export type NativeCommerceOrdersPageContext = {
  userId: string;
  holdingId: string;
  holdingSlug: string;
  companyId: string;
  branchId: string | null;
  departmentId: string | null;
  assignmentId: string;
  role: string;
  assignmentScope: string;
  scope:
    | { kind: 'holding' }
    | { kind: 'branch'; branchSlug: string }
    | { kind: 'department'; branchSlug?: string; departmentSlug: string };
  authority?: { revision: number; assignmentVersion: number };
  modules: readonly string[];
  iam?: { permissions: readonly string[] };
};

export type NativeCommerceOrdersConfiguration = Readonly<{
  privateBaseUrl: string;
  holdings: readonly Readonly<{ origin: string; holdingSlug: string; holdingId: string }>[];
  timeoutMs: number;
}>;

export type NativeCommerceOrderRow = Readonly<{
  id: string;
  company_id: string;
  branch_id: string | null;
  department_id: string | null;
  kind: 'quote' | 'sales';
  number: string;
  party_id: string;
  warehouse_id: string | null;
  currency: string;
  order_date: string;
  due_date: string;
  note: string;
  state: string;
  subtotal_minor: number;
  tax_minor: number;
  source_id: string | null;
  version: number;
  created_by: string;
  approved_by: string | null;
  created_at: number;
  updated_at: number;
  price_list_id: string | null;
  price_list_version: number | null;
  discount_reason: string | null;
  discount_approval_required: 0 | 1;
  discount_approved_by: string | null;
  discount_approved_at: number | null;
}>;

export type NativeCommerceOrderLineRow = Readonly<{
  id: string;
  order_id: string;
  product_id: string;
  description: string;
  quantity_milli: number;
  fulfilled_milli: number;
  price_minor: number;
  tax_bps: number;
  subtotal_minor: number;
  tax_minor: number;
  ordinal: number;
  list_price_minor: number | null;
  discount_bps: number;
  item_type: 'stock' | 'service';
  sku: string;
  unit: string;
  tracking_mode: 'none' | 'lot' | 'serial';
}>;

export type NativeCommerceOrderDocument = Readonly<{
  order: NativeCommerceOrderRow;
  lines: NativeCommerceOrderLineRow[];
}>;

export type NativeCommerceOrderReceipt = Readonly<{
  id: string;
  state: string;
  version: number;
  salesId: string | null;
}>;

export type NativeCommerceOrderAction = 'submit' | 'approve' | 'return' | 'cancel' | 'convert';

export type NativeCommerceOrderTransitionInput = Readonly<{
  action: NativeCommerceOrderAction;
  expectedVersion: number;
  reason?: string;
}>;

export type NativeCommerceOrderSaveLineInput = Readonly<{
  productId: string;
  quantity: string;
  price: string;
  taxBps: number;
}>;

export type NativeCommerceOrderSaveInput = Readonly<{
  kind: 'quote' | 'sales';
  partyId: string;
  warehouseId: string;
  orderDate: string;
  dueDate: string;
  note: string;
  priceListId?: string;
  discountReason?: string;
  lines: readonly NativeCommerceOrderSaveLineInput[];
  expectedVersion?: number;
}>;

export type NativeCommerceOrderSaveReceipt = Readonly<{
  id: string;
  version: number;
}>;

export type NativeCommerceServiceCompletionInput = Readonly<{
  expectedVersion: number;
  date: string;
  evidenceNote: string;
  lines: readonly Readonly<{ lineId: string; quantity: string }>[];
}>;

export type NativeCommerceGoodsAllocationInput = Readonly<{
  lotCode?: string;
  serialNumber?: string;
  quantity: string;
  manufacturedDate?: string;
  expiryDate?: string;
  disposition?: 'available' | 'quarantine';
  quarantineReason?: string;
  quarantineDueDate?: string;
}>;
export type NativeCommerceGoodsReceiptInput = Readonly<{
  expectedVersion: number;
  date: string;
  lines: readonly Readonly<{
    lineId: string;
    quantity: string;
    allocations?: readonly NativeCommerceGoodsAllocationInput[];
  }>[];
}>;

export type NativeCommerceFulfillmentReceipt = Readonly<{
  id: string;
  fulfillmentId: string;
  version: number;
  state: 'partially_fulfilled' | 'fulfilled';
}>;

export type NativeCommerceOrdersReadInput = Readonly<{
  holdingSlug: string;
  branchSlug?: string;
  departmentSlug?: string;
  expected: Readonly<{
    companyId: string;
    assignmentId: string;
    revision: number;
    assignmentVersion: number;
  }>;
}>;

export type NativeCommerceOrdersTransport = (
  method: 'GET' | 'POST',
  url: URL,
  headers: Headers,
  body: string | undefined,
  signal: AbortSignal,
) => Promise<Response>;

export class NativeCommerceOrdersError extends Error {
  readonly code: string;
  readonly status?: number;
  constructor(code: string, status?: number, message?: string) {
    super(message ?? 'The native Commerce order request could not be completed.');
    this.name = 'NativeCommerceOrdersError';
    this.code = code;
    this.status = status;
  }
}

type UnknownRow = Record<string, unknown>;

const failure = (code: string, status?: number, message?: string): never => {
  throw new NativeCommerceOrdersError(code, status, message);
};
const record = (value: unknown): value is UnknownRow =>
  value !== null && typeof value === 'object' && !Array.isArray(value);
const exact = (value: unknown, keys: readonly string[]): value is UnknownRow =>
  record(value) &&
  Object.keys(value).length === keys.length &&
  keys.every((key) => Object.hasOwn(value, key));
const text = (value: unknown, max = 4096): value is string =>
  typeof value === 'string' && new TextEncoder().encode(value).length <= max;
const identifier = (value: unknown): value is string =>
  text(value, 200) && value.length > 0 && !/[\u0000-\u001f\u007f]/.test(value);
const optionalIdentifier = (value: unknown) => value === null || identifier(value);
const optionalText = (value: unknown, max = 4096) => value === null || text(value, max);
const integer = (value: unknown, min = 0): value is number =>
  typeof value === 'number' && Number.isSafeInteger(value) && value >= min;
const optionalInteger = (value: unknown, min = 0) => value === null || integer(value, min);
const date = (value: unknown): value is string =>
  typeof value === 'string' && /^\d{4}-\d{2}-\d{2}$/.test(value);
const quantityString = (value: unknown): value is string =>
  typeof value === 'string' && /^\d+(\.\d{1,3})?$/.test(value);
const texts = (value: unknown, max = 512): value is string[] =>
  Array.isArray(value) && value.length <= max && value.every((item) => text(item, 256));
const sameSet = (left: readonly string[], right: readonly string[]) => {
  const a = new Set(left);
  const b = new Set(right);
  return a.size === b.size && [...a].every((value) => b.has(value));
};

const ORDER_KEYS = [
  'id', 'company_id', 'branch_id', 'department_id', 'kind', 'number', 'party_id',
  'warehouse_id', 'currency', 'order_date', 'due_date', 'note', 'state',
  'subtotal_minor', 'tax_minor', 'source_id', 'version', 'created_by', 'approved_by',
  'created_at', 'updated_at', 'price_list_id', 'price_list_version', 'discount_reason',
  'discount_approval_required', 'discount_approved_by', 'discount_approved_at',
] as const;
const LINE_KEYS = [
  'id', 'order_id', 'product_id', 'description', 'quantity_milli', 'fulfilled_milli',
  'price_minor', 'tax_bps', 'subtotal_minor', 'tax_minor', 'ordinal',
  'list_price_minor', 'discount_bps', 'item_type', 'sku', 'unit', 'tracking_mode',
] as const;
const kinds = new Set(['quote', 'sales']);
const states = new Set([
  'draft', 'submitted', 'confirmed', 'partially_fulfilled', 'fulfilled', 'converted', 'cancelled',
]);
const actions = new Set(['submit', 'approve', 'return', 'cancel', 'convert']);

function orderRow(value: unknown, actor: NativeCommerceOrdersPageContext): value is NativeCommerceOrderRow {
  if (!exact(value, ORDER_KEYS)) return false;
  const row = value;
  if (
    !identifier(row.id) ||
    row.company_id !== actor.companyId ||
    !optionalIdentifier(row.branch_id) ||
    (actor.branchId !== null && row.branch_id !== actor.branchId) ||
    !optionalIdentifier(row.department_id) ||
    (actor.departmentId !== null && row.department_id !== actor.departmentId) ||
    typeof row.kind !== 'string' ||
    !kinds.has(row.kind) ||
    !text(row.number, 64) ||
    row.number.length === 0 ||
    !identifier(row.party_id) ||
    !optionalIdentifier(row.warehouse_id) ||
    typeof row.currency !== 'string' ||
    !/^[A-Z]{3}$/.test(row.currency) ||
    !date(row.order_date) ||
    !date(row.due_date) ||
    row.due_date < row.order_date ||
    !text(row.note, 2000) ||
    typeof row.state !== 'string' ||
    !states.has(row.state) ||
    !integer(row.subtotal_minor) ||
    !integer(row.tax_minor) ||
    !optionalIdentifier(row.source_id) ||
    !integer(row.version, 1) ||
    !identifier(row.created_by) ||
    !optionalIdentifier(row.approved_by) ||
    !integer(row.created_at) ||
    !integer(row.updated_at) ||
    row.updated_at < row.created_at ||
    !optionalIdentifier(row.price_list_id) ||
    !optionalInteger(row.price_list_version, 1) ||
    (row.price_list_version !== null && row.price_list_id === null) ||
    !optionalText(row.discount_reason, 1000) ||
    (row.discount_approval_required !== 0 && row.discount_approval_required !== 1) ||
    !optionalIdentifier(row.discount_approved_by) ||
    !optionalInteger(row.discount_approved_at) ||
    (row.state === 'converted' && row.kind !== 'quote')
  )
    return false;
  return true;
}

function lineRow(
  value: unknown,
  order: NativeCommerceOrderRow,
  warehouse: string | null,
): value is NativeCommerceOrderLineRow {
  if (!exact(value, LINE_KEYS)) return false;
  const row = value;
  if (
    !identifier(row.id) ||
    row.order_id !== order.id ||
    !identifier(row.product_id) ||
    !text(row.description, 2000) ||
    !integer(row.quantity_milli, 1) ||
    !integer(row.fulfilled_milli) ||
    Number(row.fulfilled_milli) > Number(row.quantity_milli) ||
    !integer(row.price_minor) ||
    !integer(row.tax_bps) ||
    Number(row.tax_bps) > 10_000 ||
    !integer(row.subtotal_minor) ||
    !integer(row.tax_minor) ||
    !integer(row.ordinal) ||
    !optionalInteger(row.list_price_minor) ||
    (row.list_price_minor !== null && Number(row.list_price_minor) < Number(row.price_minor)) ||
    !integer(row.discount_bps) ||
    Number(row.discount_bps) > 10_000 ||
    (row.item_type !== 'stock' && row.item_type !== 'service') ||
    !text(row.sku, 64) ||
    row.sku.length === 0 ||
    !text(row.unit, 30) ||
    row.unit.length === 0 ||
    !['none', 'lot', 'serial'].includes(String(row.tracking_mode)) ||
    (row.item_type === 'service' && row.tracking_mode !== 'none') ||
    (row.item_type === 'stock' && warehouse === null)
  )
    return false;
  return true;
}

function document(value: unknown, actor: NativeCommerceOrdersPageContext): NativeCommerceOrderDocument {
  if (!exact(value, ['order', 'lines'])) return failure('NATIVE_COMMERCE_ORDER_RESPONSE_INVALID');
  if (!orderRow(value.order, actor)) return failure('NATIVE_COMMERCE_ORDER_RESPONSE_INVALID');
  if (!Array.isArray(value.lines) || value.lines.length < 1 || value.lines.length > 200)
    return failure('NATIVE_COMMERCE_ORDER_RESPONSE_INVALID');
  const order = value.order;
  const ids = new Set<string>();
  const products = new Set<string>();
  let previous = -1;
  let subtotal = 0;
  let tax = 0;
  const lines: NativeCommerceOrderLineRow[] = [];
  for (const candidate of value.lines) {
    if (!lineRow(candidate, order, order.warehouse_id)) return failure('NATIVE_COMMERCE_ORDER_RESPONSE_INVALID');
    const line = candidate;
    if (ids.has(line.id) || products.has(line.product_id) || Number(line.ordinal) <= previous)
      return failure('NATIVE_COMMERCE_ORDER_RESPONSE_INVALID');
    ids.add(line.id);
    products.add(line.product_id);
    previous = Number(line.ordinal);
    subtotal += Number(line.subtotal_minor);
    tax += Number(line.tax_minor);
    if (!Number.isSafeInteger(subtotal) || !Number.isSafeInteger(tax))
      return failure('NATIVE_COMMERCE_ORDER_RESPONSE_INVALID');
    lines.push(line);
  }
  if (subtotal !== Number(order.subtotal_minor) || tax !== Number(order.tax_minor))
    return failure('NATIVE_COMMERCE_ORDER_RESPONSE_INVALID');
  return { order, lines };
}

function receipt(value: unknown): NativeCommerceOrderReceipt {
  if (
    !exact(value, ['id', 'state', 'version', 'salesId']) ||
    !identifier(value.id) ||
    typeof value.state !== 'string' ||
    !states.has(value.state) ||
    !integer(value.version, 1) ||
    !optionalIdentifier(value.salesId) ||
    (value.state === 'converted') !== (value.salesId !== null)
  )
    return failure('NATIVE_COMMERCE_ORDER_RESPONSE_INVALID');
  return value as unknown as NativeCommerceOrderReceipt;
}

function saveReceipt(value: unknown): NativeCommerceOrderSaveReceipt {
  if (!exact(value, ['id', 'version']) || !identifier(value.id) || !integer(value.version, 1))
    return failure('NATIVE_COMMERCE_ORDER_RESPONSE_INVALID');
  return value as unknown as NativeCommerceOrderSaveReceipt;
}

function fulfillmentReceipt(value: unknown): NativeCommerceFulfillmentReceipt {
  if (
    !exact(value, ['id', 'fulfillmentId', 'version', 'state']) ||
    !identifier(value.id) ||
    !identifier(value.fulfillmentId) ||
    !integer(value.version, 1) ||
    (value.state !== 'partially_fulfilled' && value.state !== 'fulfilled')
  )
    return failure('NATIVE_COMMERCE_ORDER_RESPONSE_INVALID');
  return value as unknown as NativeCommerceFulfillmentReceipt;
}

function serviceCompletionInput(input: NativeCommerceServiceCompletionInput): string {
  if (
    !integer(input.expectedVersion, 1) ||
    !date(input.date) ||
    typeof input.evidenceNote !== 'string' ||
    !text(input.evidenceNote, 1000) ||
    !Array.isArray(input.lines) ||
    input.lines.length < 1 ||
    input.lines.length > 20
  )
    return failure('NATIVE_COMMERCE_ORDER_REQUEST_INVALID');
  const ids = new Set<string>();
  for (const line of input.lines) {
    if (
      !identifier(line.lineId) ||
      ids.has(line.lineId) ||
      typeof line.quantity !== 'string' ||
      !/^\d+(\.\d{1,3})?$/.test(line.quantity)
    )
      return failure('NATIVE_COMMERCE_ORDER_REQUEST_INVALID');
    ids.add(line.lineId);
  }
  return JSON.stringify({
    completionMode: 'service',
    expectedVersion: input.expectedVersion,
    date: input.date,
    evidenceNote: input.evidenceNote,
    lines: input.lines.map((line) => ({ lineId: line.lineId, quantity: line.quantity })),
  });
}

function saveInput(input: NativeCommerceOrderSaveInput): string {
  if (
    (input.kind !== 'quote' && input.kind !== 'sales') ||
    !identifier(input.partyId) ||
    typeof input.warehouseId !== 'string' ||
    !text(input.warehouseId, 200) ||
    !date(input.orderDate) ||
    !date(input.dueDate) ||
    input.dueDate < input.orderDate ||
    typeof input.note !== 'string' ||
    !text(input.note, 2000) ||
    (input.priceListId !== undefined && !identifier(input.priceListId)) ||
    (input.discountReason !== undefined && !text(input.discountReason, 1000)) ||
    !Array.isArray(input.lines) ||
    input.lines.length < 1 ||
    input.lines.length > 20 ||
    (input.expectedVersion !== undefined && !integer(input.expectedVersion, 1))
  )
    return failure('NATIVE_COMMERCE_ORDER_REQUEST_INVALID');
  const products = new Set<string>();
  for (const line of input.lines) {
    if (
      !identifier(line.productId) ||
      products.has(line.productId) ||
      typeof line.quantity !== 'string' ||
      !/^\d+(\.\d{1,3})?$/.test(line.quantity) ||
      typeof line.price !== 'string' ||
      !/^\d+(\.\d{1,2})?$/.test(line.price) ||
      !Number.isInteger(line.taxBps) ||
      line.taxBps < 0 ||
      line.taxBps > 10_000
    )
      return failure('NATIVE_COMMERCE_ORDER_REQUEST_INVALID');
    products.add(line.productId);
  }
  return JSON.stringify({
    kind: input.kind,
    partyId: input.partyId,
    warehouseId: input.warehouseId,
    orderDate: input.orderDate,
    dueDate: input.dueDate,
    note: input.note,
    ...(input.priceListId !== undefined ? { priceListId: input.priceListId } : {}),
    ...(input.discountReason !== undefined ? { discountReason: input.discountReason } : {}),
    lines: input.lines.map((line) => ({
      productId: line.productId,
      quantity: line.quantity,
      price: line.price,
      taxBps: line.taxBps,
    })),
    ...(input.expectedVersion !== undefined ? { expectedVersion: input.expectedVersion } : {}),
  });
}

function goodsReceiptInput(input: NativeCommerceGoodsReceiptInput): string {
  if (
    !integer(input.expectedVersion, 1) ||
    !date(input.date) ||
    !Array.isArray(input.lines) ||
    input.lines.length < 1 ||
    input.lines.length > 200
  )
    return failure('NATIVE_COMMERCE_ORDER_REQUEST_INVALID');
  for (const line of input.lines) {
    if (!identifier(line.lineId) || !quantityString(line.quantity)) return failure('NATIVE_COMMERCE_ORDER_REQUEST_INVALID');
    if (line.allocations !== undefined) {
      if (!Array.isArray(line.allocations) || line.allocations.length < 1 || line.allocations.length > 200)
        return failure('NATIVE_COMMERCE_ORDER_REQUEST_INVALID');
      for (const allocation of line.allocations) {
        if (!quantityString(allocation.quantity)) return failure('NATIVE_COMMERCE_ORDER_REQUEST_INVALID');
        if (allocation.lotCode !== undefined && !identifier(allocation.lotCode)) return failure('NATIVE_COMMERCE_ORDER_REQUEST_INVALID');
        if (allocation.serialNumber !== undefined && !identifier(allocation.serialNumber)) return failure('NATIVE_COMMERCE_ORDER_REQUEST_INVALID');
        for (const value of [allocation.manufacturedDate, allocation.expiryDate, allocation.quarantineDueDate])
          if (value !== undefined && !date(value)) return failure('NATIVE_COMMERCE_ORDER_REQUEST_INVALID');
        if (allocation.disposition !== undefined && !['available', 'quarantine'].includes(allocation.disposition))
          return failure('NATIVE_COMMERCE_ORDER_REQUEST_INVALID');
        if (allocation.quarantineReason !== undefined && !text(allocation.quarantineReason, 3000))
          return failure('NATIVE_COMMERCE_ORDER_REQUEST_INVALID');
      }
    }
  }
  return JSON.stringify({ completionMode: 'goods', ...input });
}

function context(
  value: unknown,
  actor: NativeCommerceOrdersPageContext,
  input: NativeCommerceOrdersReadInput,
): UnknownRow {
  if (
    !exact(value, [
      'userId', 'holdingId', 'companyId', 'branchId', 'departmentId', 'branchSlug',
      'departmentSlug', 'assignmentId', 'assignmentVersion', 'revision', 'role',
      'assignmentScopeKind', 'modules', 'permissions', 'expiresAt',
    ])
  )
    return failure('NATIVE_COMMERCE_ORDER_RESPONSE_INVALID');
  const parsed = value;
  if (
    parsed.userId !== actor.userId ||
    parsed.holdingId !== actor.holdingId ||
    parsed.companyId !== actor.companyId ||
    parsed.branchId !== actor.branchId ||
    parsed.departmentId !== actor.departmentId ||
    parsed.branchSlug !== (input.branchSlug ?? null) ||
    parsed.departmentSlug !== (input.departmentSlug ?? null) ||
    parsed.assignmentId !== actor.assignmentId ||
    parsed.assignmentVersion !== actor.authority?.assignmentVersion ||
    parsed.revision !== actor.authority?.revision ||
    parsed.role !== actor.role ||
    parsed.assignmentScopeKind !== actor.assignmentScope ||
    !integer(parsed.expiresAt, 1) ||
    !texts(parsed.modules) ||
    !sameSet(actor.modules, parsed.modules) ||
    !texts(parsed.permissions) ||
    !sameSet(actor.iam?.permissions ?? [], parsed.permissions)
  )
    return failure('NATIVE_COMMERCE_ORDER_CONTEXT_CHANGED');
  if (
    parsed.companyId !== input.expected.companyId ||
    parsed.assignmentId !== input.expected.assignmentId ||
    parsed.revision !== input.expected.revision ||
    parsed.assignmentVersion !== input.expected.assignmentVersion
  )
    return failure('NATIVE_COMMERCE_ORDER_CONTEXT_CHANGED');
  return parsed;
}

const reserved = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);

function configuration(raw: NativeCommerceOrdersConfiguration) {
  let base: URL;
  try {
    base = new URL(raw.privateBaseUrl);
  } catch {
    return failure('NATIVE_COMMERCE_ORDER_CONFIGURATION_INVALID');
  }
  const port = Number(base.port);
  if (
    base.protocol !== 'http:' ||
    !['127.0.0.1', '[::1]'].includes(base.hostname) ||
    !base.port ||
    reserved.has(port) ||
    (port >= 3200 && port <= 3299) ||
    base.username ||
    base.password ||
    raw.privateBaseUrl !== base.origin ||
    !integer(raw.timeoutMs, 250) ||
    raw.timeoutMs > 30_000 ||
    !Array.isArray(raw.holdings) ||
    raw.holdings.length < 1 ||
    raw.holdings.length > 64
  )
    return failure('NATIVE_COMMERCE_ORDER_CONFIGURATION_INVALID');
  const holdings = new Map<string, { holdingId: string; holdingSlug: string }>();
  for (const holding of raw.holdings) {
    let origin: URL;
    try {
      origin = new URL(holding.origin);
    } catch {
      return failure('NATIVE_COMMERCE_ORDER_CONFIGURATION_INVALID');
    }
    if (
      origin.protocol !== 'https:' ||
      holding.origin !== origin.origin ||
      (origin.port !== '' &&
        !(
          process.env.NODE_ENV !== 'production' &&
          origin.port === '3443' &&
          origin.hostname.endsWith('.erpx.localhost')
        )) ||
      origin.username ||
      origin.password ||
      !identifier(holding.holdingId) ||
      !text(holding.holdingSlug, 128) ||
      !/^[a-z0-9_-]+$/.test(holding.holdingSlug) ||
      holdings.has(origin.host)
    )
      return failure('NATIVE_COMMERCE_ORDER_CONFIGURATION_INVALID');
    holdings.set(origin.host, {
      holdingId: holding.holdingId,
      holdingSlug: holding.holdingSlug,
    });
  }
  return { base, holdings, timeout: raw.timeoutMs };
}

function transportCall(
  transport: NativeCommerceOrdersTransport,
  base: URL,
  holdings: Map<string, { holdingId: string; holdingSlug: string }>,
  incoming: Pick<Headers, 'get'>,
  input: NativeCommerceOrdersReadInput,
  section: 'orders' | 'purchase-orders',
  path: string,
  method: 'GET' | 'POST',
  body: string | undefined,
  key: string | undefined,
  timeout: number,
  signal?: AbortSignal,
): Promise<Response> {
  const host = incoming.get('host');
  const holding = host ? holdings.get(host) : undefined;
  if (!holding || holding.holdingSlug !== input.holdingSlug)
    return Promise.reject(new NativeCommerceOrdersError('NATIVE_COMMERCE_ORDER_AUTHORITY_INVALID'));
  const headers = new Headers({
    host: host!,
    accept: 'application/json',
    'cache-control': 'no-cache',
  });
  const cookie = incoming.get('cookie');
  if (cookie) {
    if (new TextEncoder().encode(cookie).length > 8192)
      return Promise.reject(new NativeCommerceOrdersError('NATIVE_COMMERCE_ORDER_COOKIE_LIMIT'));
    headers.set('cookie', cookie);
  }
  for (const [header, slug] of [
    ['x-erpx-workspace-branch', input.branchSlug],
    ['x-erpx-workspace-department', input.departmentSlug],
  ] as const) {
    if (slug !== undefined) {
      if (!text(slug, 128) || !/^[a-z0-9_-]+$/.test(slug))
        return Promise.reject(new NativeCommerceOrdersError('NATIVE_COMMERCE_ORDER_REQUEST_INVALID'));
      headers.set(header, slug);
    }
  }
  if (
    !identifier(input.expected.companyId) ||
    !identifier(input.expected.assignmentId) ||
    !integer(input.expected.revision) ||
    !integer(input.expected.assignmentVersion, 1)
  )
    return Promise.reject(new NativeCommerceOrdersError('NATIVE_COMMERCE_ORDER_REQUEST_INVALID'));
  headers.set('x-erpx-context-company', input.expected.companyId);
  headers.set('x-erpx-context-assignment', input.expected.assignmentId);
  headers.set('x-erpx-context-revision', String(input.expected.revision));
  headers.set('x-erpx-context-assignment-version', String(input.expected.assignmentVersion));
  if (method === 'POST') {
    if (!key || !text(key, 180) || key.length < 8 || !/^[A-Za-z0-9:_.-]+$/.test(key))
      return Promise.reject(new NativeCommerceOrdersError('NATIVE_COMMERCE_ORDER_REQUEST_INVALID'));
    if (body === undefined)
      return Promise.reject(new NativeCommerceOrdersError('NATIVE_COMMERCE_ORDER_REQUEST_INVALID'));
    headers.set('content-type', 'application/json');
    headers.set('idempotency-key', key);
  }
  const url = new URL(
    `/api/v1/holdings/${encodeURIComponent(input.holdingSlug)}/commerce/${section}${path ? `/${path}` : ''}`,
    base,
  );
  const controller = new AbortController();
  const cancel = () => controller.abort();
  signal?.addEventListener('abort', cancel, { once: true });
  if (signal?.aborted) controller.abort();
  const timer = setTimeout(cancel, timeout);
  return transport(method, url, headers, body, controller.signal).finally(() => {
    clearTimeout(timer);
    signal?.removeEventListener('abort', cancel);
  });
}

async function boundedBytes(response: Response, limit: number): Promise<Uint8Array> {
  if (!response.body) return new Uint8Array();
  const reader = response.body.getReader();
  const chunks: Uint8Array[] = [];
  let bytes = 0;
  try {
    for (;;) {
      const next = await reader.read();
      if (next.done) break;
      bytes += next.value.byteLength;
      if (bytes > limit) {
        await reader.cancel();
        return failure('NATIVE_COMMERCE_ORDER_RESPONSE_LIMIT');
      }
      chunks.push(next.value);
    }
  } finally {
    reader.releaseLock();
  }
  const data = new Uint8Array(bytes);
  let offset = 0;
  for (const chunk of chunks) {
    data.set(chunk, offset);
    offset += chunk.length;
  }
  return data;
}

async function boundedJson(response: Response): Promise<unknown> {
  if (response.redirected) {
    void response.body?.cancel().catch(() => undefined);
    return failure('NATIVE_COMMERCE_ORDER_UNAVAILABLE', response.status);
  }
  const contentType = response.headers.get('content-type')?.split(';')[0]?.trim();
  if (response.status !== 200) {
    const body = await boundedBytes(response, 64 * 1024);
    let code: string | undefined;
    let message: string | undefined;
    if (contentType === 'application/json') {
      try {
        const value: unknown = JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(body));
        if (record(value) && record(value.error)) {
          if (text(value.error.code, 200)) code = value.error.code;
          if (text(value.error.message, 2000)) message = value.error.message;
        }
      } catch {
        // A malformed owner error keeps the bounded transport failure.
      }
    }
    return failure(code ?? 'NATIVE_COMMERCE_ORDER_UNAVAILABLE', response.status, message);
  }
  if (contentType !== 'application/json') {
    void response.body?.cancel().catch(() => undefined);
    return failure('NATIVE_COMMERCE_ORDER_RESPONSE_INVALID');
  }
  const data = await boundedBytes(response, 2 * 1024 * 1024);
  try {
    return JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(data));
  } catch {
    return failure('NATIVE_COMMERCE_ORDER_RESPONSE_INVALID');
  }
}

export function nativeCommerceOrdersAvailable(): boolean {
  return process.env.ERPX_NATIVE_COMMERCE_ORDER_CONFIG !== undefined;
}

export function nativeCommerceOrdersConfiguration(
  raw: string,
): NativeCommerceOrdersConfiguration {
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    throw new NativeCommerceOrdersError('NATIVE_COMMERCE_ORDER_CONFIGURATION_INVALID');
  }
  if (
    !record(value) ||
    !text(value.privateBaseUrl) ||
    !integer(value.timeoutMs) ||
    !Array.isArray(value.holdings) ||
    !value.holdings.every(
      (holding: unknown) =>
        record(holding) &&
        text(holding.origin) &&
        text(holding.holdingSlug) &&
        text(holding.holdingId),
    )
  )
    return failure('NATIVE_COMMERCE_ORDER_CONFIGURATION_INVALID');
  return value as unknown as NativeCommerceOrdersConfiguration;
}

export function nativeCommerceOrdersFailure(
  error: unknown,
): { status: number; code: string; message: string } | undefined {
  if (!(error instanceof NativeCommerceOrdersError)) return undefined;
  if (error.code === 'NATIVE_COMMERCE_ORDER_CONTEXT_CHANGED')
    return {
      status: 409,
      code: 'WORK_CONTEXT_CHANGED',
      message: 'Your work role changed. Reload and review before retrying.',
    };
  if (error.status === 401)
    return { status: 401, code: 'UNAUTHENTICATED', message: 'Sign in to continue.' };
  if (error.status === 403)
    return {
      status: 403,
      code: error.code === 'NATIVE_COMMERCE_ORDER_UNAVAILABLE' ? 'FORBIDDEN' : error.code,
      message: error.message,
    };
  if (error.status === 404)
    return {
      status: 404,
      code: error.code === 'NATIVE_COMMERCE_ORDER_UNAVAILABLE' ? 'NOT_FOUND' : error.code,
      message: error.message,
    };
  if (error.status === 409)
    return { status: 409, code: error.code, message: error.message };
  if (error.status === 422)
    return { status: 422, code: error.code, message: error.message };
  if (error.status === 503)
    return {
      status: 503,
      code: 'NATIVE_COMMERCE_ORDER_UNAVAILABLE',
      message: 'The native Commerce order service is unavailable.',
    };
  if (error.code === 'NATIVE_COMMERCE_ORDER_CONFIGURATION_INVALID')
    return {
      status: 503,
      code: 'NATIVE_COMMERCE_ORDER_UNAVAILABLE',
      message: 'The native Commerce order reader is unavailable.',
    };
  if (error.code === 'NATIVE_COMMERCE_ORDER_REQUEST_INVALID')
    return {
      status: 422,
      code: 'COMMERCE_ORDER_REQUEST_INVALID',
      message: 'Review the Commerce order request.',
    };
  return {
    status: 503,
    code: 'NATIVE_COMMERCE_ORDER_UNAVAILABLE',
    message: 'The native Commerce order reader is unavailable.',
  };
}

export function createNativeCommerceOrdersClient(
  raw: NativeCommerceOrdersConfiguration,
  transport: NativeCommerceOrdersTransport = directHttp,
) {
  const config = configuration(raw);
  const run = async (
    incoming: Pick<Headers, 'get'>,
    actor: NativeCommerceOrdersPageContext,
    input: NativeCommerceOrdersReadInput,
    section: 'orders' | 'purchase-orders',
    path: string,
    method: 'GET' | 'POST',
    body: string | undefined,
    key: string | undefined,
    signal?: AbortSignal,
  ) => {
    if (!actor.authority || !actor.iam || actor.holdingSlug !== input.holdingSlug)
      return failure('NATIVE_COMMERCE_ORDER_CONTEXT_CHANGED');
    const response = await transportCall(
      transport,
      config.base,
      config.holdings,
      incoming,
      input,
      section,
      path,
      method,
      body,
      key,
      config.timeout,
      signal,
    ).catch((error: unknown) => {
      if (error instanceof NativeCommerceOrdersError) throw error;
      return failure('NATIVE_COMMERCE_ORDER_UNAVAILABLE');
    });
    return boundedJson(response);
  };
  return {
    async detail(
      incoming: Pick<Headers, 'get'>,
      actor: NativeCommerceOrdersPageContext,
      orderId: string,
      input: NativeCommerceOrdersReadInput,
      signal?: AbortSignal,
    ): Promise<NativeCommerceOrderDocument> {
      if (!identifier(orderId)) return failure('NATIVE_COMMERCE_ORDER_REQUEST_INVALID');
      const value = await run(
        incoming,
        actor,
        input,
        'orders',
        encodeURIComponent(orderId),
        'GET',
        undefined,
        undefined,
        signal,
      );
      if (!exact(value, ['data', 'context'])) return failure('NATIVE_COMMERCE_ORDER_RESPONSE_INVALID');
      const data = document(value.data, actor);
      context(value.context, actor, input);
      return data;
    },
    async transition(
      incoming: Pick<Headers, 'get'>,
      actor: NativeCommerceOrdersPageContext,
      orderId: string,
      command: NativeCommerceOrderTransitionInput,
      key: string,
      input: NativeCommerceOrdersReadInput,
      signal?: AbortSignal,
    ): Promise<NativeCommerceOrderReceipt> {
      if (
        !identifier(orderId) ||
        !actions.has(command.action) ||
        !integer(command.expectedVersion, 1) ||
        (command.reason !== undefined && !text(command.reason, 1000))
      )
        return failure('NATIVE_COMMERCE_ORDER_REQUEST_INVALID');
      const value = await run(
        incoming,
        actor,
        input,
        'orders',
        `${encodeURIComponent(orderId)}/transition`,
        'POST',
        JSON.stringify({
          action: command.action,
          expectedVersion: command.expectedVersion,
          ...(command.reason !== undefined ? { reason: command.reason } : {}),
        }),
        key,
        signal,
      );
      if (!exact(value, ['data', 'context'])) return failure('NATIVE_COMMERCE_ORDER_RESPONSE_INVALID');
      const data = receipt(value.data);
      context(value.context, actor, input);
      return data;
    },
    async save(
      incoming: Pick<Headers, 'get'>,
      actor: NativeCommerceOrdersPageContext,
      orderId: string | null,
      command: NativeCommerceOrderSaveInput,
      key: string,
      input: NativeCommerceOrdersReadInput,
      signal?: AbortSignal,
    ): Promise<NativeCommerceOrderSaveReceipt> {
      if (orderId !== null && !identifier(orderId))
        return failure('NATIVE_COMMERCE_ORDER_REQUEST_INVALID');
      const value = await run(
        incoming,
        actor,
        input,
        'orders',
        orderId === null ? '' : encodeURIComponent(orderId),
        'POST',
        saveInput(command),
        key,
        signal,
      );
      if (!exact(value, ['data', 'context'])) return failure('NATIVE_COMMERCE_ORDER_RESPONSE_INVALID');
      const data = saveReceipt(value.data);
      context(value.context, actor, input);
      return data;
    },
    async fulfill(
      incoming: Pick<Headers, 'get'>,
      actor: NativeCommerceOrdersPageContext,
      orderId: string,
      command: NativeCommerceServiceCompletionInput,
      key: string,
      input: NativeCommerceOrdersReadInput,
      signal?: AbortSignal,
    ): Promise<NativeCommerceFulfillmentReceipt> {
      if (!identifier(orderId)) return failure('NATIVE_COMMERCE_ORDER_REQUEST_INVALID');
      const value = await run(
        incoming,
        actor,
        input,
        'orders',
        `${encodeURIComponent(orderId)}/fulfill`,
        'POST',
        serviceCompletionInput(command),
        key,
        signal,
      );
      if (!exact(value, ['data', 'context'])) return failure('NATIVE_COMMERCE_ORDER_RESPONSE_INVALID');
      const data = fulfillmentReceipt(value.data);
      context(value.context, actor, input);
      return data;
    },
    async goodsReceipt(
      incoming: Pick<Headers, 'get'>,
      actor: NativeCommerceOrdersPageContext,
      orderId: string,
      command: NativeCommerceGoodsReceiptInput,
      key: string,
      input: NativeCommerceOrdersReadInput,
      signal?: AbortSignal,
    ): Promise<NativeCommerceFulfillmentReceipt> {
      if (!identifier(orderId)) return failure('NATIVE_COMMERCE_ORDER_REQUEST_INVALID');
      const value = await run(
        incoming,
        actor,
        input,
        'purchase-orders',
        `${encodeURIComponent(orderId)}/goods-receipts`,
        'POST',
        goodsReceiptInput(command),
        key,
        signal,
      );
      if (!exact(value, ['data', 'context'])) return failure('NATIVE_COMMERCE_ORDER_RESPONSE_INVALID');
      const data = fulfillmentReceipt(value.data);
      context(value.context, actor, input);
      return data;
    },
  };
}

/** Fresh bounded socket; bypasses patched fetch/HMR and ambient proxies. */
export function directHttp(
  method: 'GET' | 'POST',
  url: URL,
  headers: Headers,
  body: string | undefined,
  signal: AbortSignal,
): Promise<Response> {
  return new Promise((resolve, reject) => {
    const agent = new Agent({
      keepAlive: false,
      maxSockets: 1,
      maxTotalSockets: 1,
      proxyEnv: {} as import('node:http').ProxyEnv,
    });
    const outgoing = request(
      url,
      {
        method,
        agent,
        headers: { ...Object.fromEntries(headers) },
        signal,
        maxHeaderSize: 8192,
      },
      (incoming) => {
        incoming.once('close', () => agent.destroy());
        try {
          const status = incoming.statusCode ?? 502;
          const values = new Headers();
          for (let index = 0; index < incoming.rawHeaders.length; index += 2)
            values.append(incoming.rawHeaders[index]!, incoming.rawHeaders[index + 1]!);
          resolve(
            new Response(Readable.toWeb(incoming) as unknown as ReadableStream<Uint8Array>, {
              status: status >= 200 && status <= 599 ? status : 502,
              headers: values,
            }),
          );
        } catch (error) {
          incoming.destroy();
          agent.destroy();
          reject(error);
        }
      },
    );
    outgoing.once('error', (error) => {
      agent.destroy();
      reject(error);
    });
    outgoing.end(body);
  });
}
