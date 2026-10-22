/** Owner-native joined facts retain the caller's single SQL snapshot and transaction. */export const productionReadProjection={
  "executiveBranch": "LEFT JOIN organization_nodes branch ON branch.id=s.branch_id AND branch.company_id=s.company_id"
} as const;
