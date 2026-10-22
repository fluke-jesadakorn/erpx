/** Owner-native joined facts retain the caller's single SQL snapshot and transaction. */export const productionReadProjection={
  "operationResource": "JOIN production_resources r ON r.id=op.resource_id",
  "capacityResource": "JOIN production_resources r ON r.id=o.resource_id"
} as const;
