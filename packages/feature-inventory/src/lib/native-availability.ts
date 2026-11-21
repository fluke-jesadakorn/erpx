import type {
  NativeAvailability,
  NativeAvailabilityRow,
} from '../adapters/server/native-availability';
import type { NativeStockRegister } from '../adapters/server/native-stock-register';

export type NativeStockRegisterView = {
  rows: NativeAvailabilityRow[];
  currency: string;
  native: true;
};

/**
 * The configured native register is authoritative even when it returns no
 * rows; only an absent register falls back to availability. Returning
 * undefined keeps the retained Commerce rows out of the native decision and
 * prevents a scoped empty register from displaying foreign/legacy stock.
 */
export function selectNativeStockRegisterView(
  register: NativeStockRegister | undefined,
  availability: NativeAvailability | undefined,
): NativeStockRegisterView | undefined {
  if (register) return { rows: register.stock, currency: register.currency, native: true };
  if (availability) return { rows: availability.rows, currency: availability.currency, native: true };
  return undefined;
}

// Public page-facing alias for the server-only native availability read.
export {
  nativeInventoryAvailability,
  nativeInventoryAvailabilityFromHeaders,
  nativeInventoryMovementsFromHeaders,
  nativeInventoryReceiptFromHeaders,
  nativeInventoryStockRegisterFromHeaders,
  NativeStockRegisterError,
  type NativeReceiptSource,
  type NativeStockCount,
  type NativeStockLot,
  type NativeStockRegister,
  type NativeStockTotals,
  NativeAvailabilityError,
  type NativeAvailability,
  type NativeAvailabilityContext,
  type NativeAvailabilityRow,
  type NativeMovementRow,
  type NativeMovements,
} from '../adapters/server/native-availability';
