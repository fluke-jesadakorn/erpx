import 'server-only';
import { employeeRequestsProjectionSql } from './platform-owner-projection-sources';
/** Fixed explicit-column owning projections; retains the consumer's supplied statement snapshot. */
export function identityVerifiedContextProjections(){return {
  "records_employee_requests_facts_employee_requests_r_from": `FROM (${employeeRequestsProjectionSql}) r`
} as const;}
