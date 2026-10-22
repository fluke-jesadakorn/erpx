export const inboxAudiencePeopleJoins=`LEFT JOIN hr_people p ON p.company_id=a.company_id AND p.user_id=a.user_id AND p.merged_into_id IS NULL
 LEFT JOIN hr_engagements h ON h.company_id=p.company_id AND h.person_id=p.id AND h.status='active'`;
