import "server-only";
import {ppRd45EngagementProjection,ppRd45PeopleProjection} from './pp-read-tail-r45-sources';
/** Owner-defined, read-only row projections embedded in Calendar's single native statement. */
export const calendarPeopleProjectionSources={
 people:ppRd45PeopleProjection("id,user_id,company_id,legal_name,merged_into_id"),
 engagements:ppRd45EngagementProjection("person_id,company_id,status,branch_id,department_id")
} as const;
