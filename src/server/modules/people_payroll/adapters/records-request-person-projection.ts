export const requestActorNameSql = "COALESCE((SELECT legal_name FROM hr_people p WHERE p.company_id=e.company_id AND p.user_id=e.actor_id AND p.merged_into_id IS NULL LIMIT 1),'Company member')";
