import 'server-only';
/** Fixed explicit-column owning projections; retains the consumer's supplied statement snapshot. */
export function peoplePayrollFinalOwnerProjections(){return {
  "attachments_sensitive_facts_benefit_plans_p_from": "FROM (SELECT \"id\",\"company_id\",\"name\",\"provider\",\"coverage_type\",\"currency\",\"employee_deduction_minor\",\"employer_cost_minor\",\"effective_from\",\"effective_to\",\"status\",\"created_by\",\"reviewed_by\",\"version\",\"created_at\",\"updated_at\",\"required_eligibility_code\" FROM benefit_plans) p"
} as const;}
