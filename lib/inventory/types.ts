export type StockMoveKind =
  | 'receipt'
  | 'shipment'
  | 'customer_return'
  | 'vendor_return'
  | 'transfer'
  | 'adjustment'
  | 'count'
  | 'write_down'
  | 'write_down_reversal'
  | 'recost';

export interface StockMoveLine {
  productId: number;
  quantity: number;
  unitCode: string;
  unitCostThb?: number;
  fromWarehouseId?: number | null;
  fromBinId?: number | null;
  toWarehouseId?: number | null;
  toBinId?: number | null;
  lotId?: number | null;
  sourceLineId?: string | null;
  metadata?: Record<string, unknown>;
}

export interface StockMove {
  kind: StockMoveKind;
  movementDate: string;
  sourceType: string;
  sourceId: string;
  sourceEventKey: string;
  branchId: number;
  lines: StockMoveLine[];
  metadata?: Record<string, unknown>;
}

export interface StockMoveResult {
  movementId: number;
  movementNo: string;
  journalId: number | null;
  status: 'posted';
}

export interface StockWriteDown {
  productId: number;
  warehouseId: number;
  lotId?: number | null;
  movementDate: string;
  amountThb: number;
  reason: string;
  sourceEventKey: string;
  branchId: number;
}
