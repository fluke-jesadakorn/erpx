import "server-only";
/** Owner-defined, read-only row projections embedded in Calendar's single native statement. */
export const calendarPeopleProjectionSources={
 people:"SELECT id,user_id,company_id,legal_name,merged_into_id FROM hr_people",
 engagements:"SELECT person_id,company_id,status,branch_id,department_id FROM hr_engagements"
} as const;
