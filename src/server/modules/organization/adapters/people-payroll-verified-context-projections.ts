import 'server-only';
/** Fixed explicit-column owning projections; retains the consumer's supplied statement snapshot. */
export function peopleVerifiedContextProjections(){return {
  "people_payroll_position_pay_defaults_from": "FROM (SELECT \"id\",\"company_id\",\"position_id\",\"version\",\"pay_group_id\",\"currency\",\"monthly_amount_minor\",\"created_by\",\"created_at\" FROM position_pay_defaults) position_pay_defaults",
  "people_payroll_level_leave_defaults_d_from": "FROM (SELECT \"id\",\"company_id\",\"level_id\",\"version\",\"policy_version_id\",\"created_by\",\"created_at\" FROM level_leave_defaults) d",
  "people_payroll_level_leave_defaults_x_from": "FROM (SELECT \"id\",\"company_id\",\"level_id\",\"version\",\"policy_version_id\",\"created_by\",\"created_at\" FROM level_leave_defaults) x"
} as const;}
