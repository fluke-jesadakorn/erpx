import {ppRd36NumberedTwoPred} from './pp-read-tail-r36-sources';
/** Closed projections for the original IAM provisioning snapshot and assignment fence. */
export function technologyVerifiedPersonUsersProjection(){return ppRd36NumberedTwoPred("user_id", "hr_people", "company_id", "id");}
export function technologyRestorationAssignmentProjection(){return 'people_person_user($1,$2,user_id) AND revoked_at IS NULL AND NOT EXISTS(SELECT 1 FROM people_provision_grants g WHERE g.operation_id=$4 AND g.assignment_id=assignments.id)';}
