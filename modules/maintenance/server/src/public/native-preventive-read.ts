/**
 * Public surface of the native preventive-plan read transport. The owner route
 * `/maintenance/plans` is the sole read authority; no TypeScript SQL remains.
 */
export {
  preventivePlans,
  preventivePlanDetail,
  preventivePlanWork,
  nativePreventiveReadConfigured,
  type NativePreventivePlan,
  type NativePreventivePlanDetail,
} from '../adapters/native-preventive-read';
