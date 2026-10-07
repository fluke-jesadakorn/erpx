/** Fixed journal aliases preserve Company membership and unlinked-account fallback in one snapshot. */
export function financeJournalPeopleFacts(){return {
 createdBy:"COALESCE((SELECT p.legal_name FROM hr_people p WHERE p.user_id=j.created_by AND p.company_id=j.company_id AND p.merged_into_id IS NULL LIMIT 1),'Unlinked account') created_by_name",
 postedBy:"COALESCE((SELECT p.legal_name FROM hr_people p WHERE p.user_id=j.posted_by AND p.company_id=j.company_id AND p.merged_into_id IS NULL LIMIT 1),'Unlinked account') posted_by_name",
} as const;}
