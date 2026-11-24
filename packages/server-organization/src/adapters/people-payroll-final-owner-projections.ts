import 'server-only';
/** Canonical alias=l staff_levels projection shared by the people/payroll final-owner projection source map. */
const STAFF_LEVELS_L_JOIN_SOURCE = 'JOIN (SELECT "id","company_id","code","name","rank","active","version","created_at","updated_at","level_number" FROM staff_levels) l';
/** Canonical alias=n organization_nodes projection shared by the people/payroll final-owner projection source map. */
const ORG_NODES_N_JOIN_SOURCE = 'JOIN (SELECT "id","kind","slug","display_name","parent_id","company_id","branch_id","status","hierarchy_revision" FROM organization_nodes) n';
/** Canonical alias=c organization_nodes projection shared by the people/payroll final-owner projection source map. */
const ORG_NODES_C_JOIN_SOURCE = 'JOIN (SELECT "id","kind","slug","display_name","parent_id","company_id","branch_id","status","hierarchy_revision" FROM organization_nodes) c';
/** Canonical alias=op organization_positions projection shared by the people/payroll final-owner projection source map. */
const ORG_POSITIONS_OP_JOIN_SOURCE = 'JOIN (SELECT "id","company_id","branch_id","department_id","title","level_id","reports_to_position_id","is_ceo","active","version","created_by","created_at","updated_at","requirements","roster_required" FROM organization_positions) op';
/** Canonical alias=organization_nodes FROM projection shared by the people/payroll final-owner projection source map. */
const ORG_NODES_FROM_SOURCE = 'FROM (SELECT "id","kind","slug","display_name","parent_id","company_id","branch_id","status","hierarchy_revision" FROM organization_nodes) organization_nodes';
/** Canonical alias=p organization_positions JOIN projection shared by the people/payroll final-owner projection source map. */
const ORG_POSITIONS_P_JOIN_SOURCE = 'JOIN (SELECT "id","company_id","branch_id","department_id","title","level_id","reports_to_position_id","is_ceo","active","version","created_by","created_at","updated_at","requirements","roster_required" FROM organization_positions) p';
/** Fixed explicit-column owning projections; retains the consumer's supplied statement snapshot. */
export function peoplePayrollFinalOwnerProjections(){return {
  "attachments_finance_facts_organization_nodes_from": ORG_NODES_FROM_SOURCE,
  "calendar_commands_facts_organization_positions_p_join": ORG_POSITIONS_P_JOIN_SOURCE,
  "calendar_lifecycle_facts_organization_positions_p_join": ORG_POSITIONS_P_JOIN_SOURCE,
  "executive_engagement_source_staff_levels_l_join": STAFF_LEVELS_L_JOIN_SOURCE,
  "executive_metric_projections_organization_nodes_from": ORG_NODES_FROM_SOURCE,
  "executive_metric_projections_staff_levels_from": "FROM (SELECT \"id\",\"company_id\",\"code\",\"name\",\"rank\",\"active\",\"version\",\"created_at\",\"updated_at\",\"level_number\" FROM staff_levels) staff_levels",
  "executive_report_sources_staff_levels_l_join": STAFF_LEVELS_L_JOIN_SOURCE,
  "executive_report_sources_organization_nodes_c_join": ORG_NODES_C_JOIN_SOURCE,
  "executive_report_sources_organization_nodes_n_join": ORG_NODES_N_JOIN_SOURCE,
  "executive_report_sources_organization_nodes_b_join": "JOIN (SELECT \"id\",\"kind\",\"slug\",\"display_name\",\"parent_id\",\"company_id\",\"branch_id\",\"status\",\"hierarchy_revision\" FROM organization_nodes) b",
  "operations_staff_settings_facts_organization_nodes_n_from": "FROM (SELECT \"id\",\"kind\",\"slug\",\"display_name\",\"parent_id\",\"company_id\",\"branch_id\",\"status\",\"hierarchy_revision\" FROM organization_nodes) n",
  "operations_staff_settings_facts_department_heads_h_join": "JOIN (SELECT \"department_id\",\"company_id\",\"person_id\",\"version\",\"updated_by\",\"updated_at\" FROM department_heads) h",
  "operations_staff_settings_facts_organization_positions_from": "FROM (SELECT \"id\",\"company_id\",\"branch_id\",\"department_id\",\"title\",\"level_id\",\"reports_to_position_id\",\"is_ceo\",\"active\",\"version\",\"created_by\",\"created_at\",\"updated_at\",\"requirements\",\"roster_required\" FROM organization_positions) organization_positions",
  "organization_authority_people_organization_positions_p_from": "FROM (SELECT \"id\",\"company_id\",\"branch_id\",\"department_id\",\"title\",\"level_id\",\"reports_to_position_id\",\"is_ceo\",\"active\",\"version\",\"created_by\",\"created_at\",\"updated_at\",\"requirements\",\"roster_required\" FROM organization_positions) p",
  "organization_roster_facts_staff_levels_l_join": STAFF_LEVELS_L_JOIN_SOURCE,
  "people_evaluation_organization_nodes_n_join": ORG_NODES_N_JOIN_SOURCE,
  "profile_work_organization_nodes_c_join": ORG_NODES_C_JOIN_SOURCE,
  "profile_work_organization_positions_op_join": ORG_POSITIONS_OP_JOIN_SOURCE,
  "spending_templates_spending_approval_templates_from": "FROM (SELECT \"company_id\",\"version\",\"exceptions_json\",\"reason\",\"updated_by\",\"updated_at\" FROM spending_approval_templates) spending_approval_templates",
  "workflow_approvals_facts_staff_levels_l_join": STAFF_LEVELS_L_JOIN_SOURCE,
  "workflow_flow_routing_facts_organization_nodes_n_join": ORG_NODES_N_JOIN_SOURCE,
  "people_directory_repository_organization_positions_op_join": ORG_POSITIONS_OP_JOIN_SOURCE,
  "people_directory_repository_organization_nodes_c_join": ORG_NODES_C_JOIN_SOURCE
} as const;}
