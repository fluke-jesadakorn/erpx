/** Closed projections for the original IAM provisioning snapshot and assignment fence. */
export function technologyVerifiedPersonUsersProjection(){return 'SELECT user_id FROM hr_people WHERE company_id=$1 AND id=$2';}
export function technologyRestorationAssignmentProjection(){return 'people_person_user($1,$2,user_id) AND revoked_at IS NULL AND NOT EXISTS(SELECT 1 FROM people_provision_grants g WHERE g.operation_id=$4 AND g.assignment_id=assignments.id)';}
