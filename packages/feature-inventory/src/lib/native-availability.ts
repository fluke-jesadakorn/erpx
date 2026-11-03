// Public page-facing alias for the server-only native availability read.
export {
  nativeInventoryAvailability,
  nativeInventoryAvailabilityFromHeaders,
  NativeAvailabilityError,
  type NativeAvailability,
  type NativeAvailabilityContext,
  type NativeAvailabilityRow,
} from '../adapters/server/native-availability';
