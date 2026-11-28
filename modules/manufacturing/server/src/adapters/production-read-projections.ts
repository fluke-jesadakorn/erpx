/** Owner-native joined facts retain the caller's single SQL snapshot and transaction. */export const productionReadProjection={
  "scenarioSpecification": "JOIN manufacturing_specifications s ON s.id=p.specification_id",
  "executiveSpecification": "LEFT JOIN manufacturing_specifications spec ON spec.id=s.specification_id AND spec.company_id=s.company_id",
  "resourceReleaseOrder": "JOIN manufacturing_orders o ON o.recommendation_id=r.id",
  "materialSpecification": "JOIN manufacturing_specification_lines ml ON ml.product_id=l.product_id",
  "routingOperations": "FROM manufacturing_specification_operations o"
} as const;
