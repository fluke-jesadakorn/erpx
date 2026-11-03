// Public page-facing alias for the server-only native availability read.
export {
  nativeInventoryAvailability,
  nativeInventoryAvailabilityFromHeaders,
  nativeInventoryMovementsFromHeaders,
  NativeAvailabilityError,
  type NativeAvailability,
  type NativeAvailabilityContext,
  type NativeAvailabilityRow,
  type NativeMovementRow,
  type NativeMovements,
} from '../adapters/server/native-availability';
