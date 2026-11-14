/** Bounded supplier promise history rows returned by the Procurement owner. */
export type NativeSupplierPromiseHistoryRow = Readonly<{
  id: string;
  order_id: string;
  company_id: string;
  order_version: number;
  supplier_id: string;
  currency: string;
  required_date: string;
  kind: 'confirmed' | 'withdrawn';
  confirmation_date: string;
  supplier_reference: string;
  evidence: string;
  recorded_by: string;
  created_at: number;
}>;

export type NativeSupplierPromiseLineRow = Readonly<{
  id: string;
  promise_id: string;
  order_line_id: string;
  ordinal: number;
  quantity_milli: number;
  fulfilled_snapshot_milli: number;
  promised_date: string;
}>;

export type NativeOpenPromiseRow = Readonly<{
  id: string;
  promise_id: string;
  order_id: string;
  company_id: string;
  branch_id: string;
  department_id: string | null;
  order_number: string;
  order_state:
    | 'draft'
    | 'submitted'
    | 'confirmed'
    | 'partially_fulfilled'
    | 'fulfilled'
    | 'converted'
    | 'cancelled';
  order_line_id: string;
  ordinal: number;
  quantity_milli: number;
  fulfilled_snapshot_milli: number;
  promised_date: string;
  product_id: string;
  description: string;
  fulfilled_milli: number;
  prior_quantity: number;
  remaining_milli: number;
}>;

export type NativeSupplierPromiseHistory = Readonly<{
  order_id: string;
  history: NativeSupplierPromiseHistoryRow[];
  lines: NativeSupplierPromiseLineRow[];
  open: NativeOpenPromiseRow[];
}>;
