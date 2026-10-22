/** Fixed owner facts used by native joined Inventory projections; application ports return typed rows. */
export const inventoryReadProjection={
decisionActor:"LEFT JOIN hr_people p ON p.user_id=d.actor_id AND p.company_id=? AND p.merged_into_id IS NULL",
historyActor:"LEFT JOIN hr_people p ON p.user_id=a.actor_id AND p.company_id=? AND p.merged_into_id IS NULL"
} as const;
