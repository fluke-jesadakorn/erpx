/** Native joined facts preserve the caller's transaction and original SQL snapshot. */ export const projectReadProjection = {
    "milestoneOwner": "LEFT JOIN hr_people p ON p.company_id=m.company_id AND p.user_id=m.owner_user_id AND p.merged_into_id IS NULL"
} as const;
