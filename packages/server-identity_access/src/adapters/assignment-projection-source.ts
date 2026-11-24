/** Fixed projections shared by the owner snapshot sources. Each keeps the
 * byte-identical clause text (JOIN/FROM prefix and alias included) of its callers. */
export const assignmentsProjection = 'SELECT "id", "user_id", "role", "modules_json", "company_id", "branch_id", "valid_from", "valid_until", "revoked_at", "version", "scope_kind", "scope_node_id", "parent_id", "source", "source_id" FROM assignments';
export const assignmentsProjectionCompact = assignmentsProjection.replace(/", "/g, '","');
export const iamMembersProjection = 'SELECT "company_id","user_id","principal_id","person_id","status","login_policy","connection_id","identity_id","version","emergency_until" FROM iam_members';
export const iamIdentitiesProjection = 'SELECT "id","company_id","connection_id","external_id","username","name","email","enabled","groups_json","observed_at","user_id","person_id" FROM iam_identities';
export const peopleDirectoryOperationsProjection = 'SELECT "id","company_id","connection_id","connection_version","state_version","version","status","definition","evidence","review_id","created_by","assignment_id","created_at","updated_at","attempts","next_attempt_at","last_error","result","approval_mode","mfa_session_id","steps","provision_id" FROM people_directory_operations';
/** Fixed explicit-column GPO rules projection shared by the Platform owner
 * snapshot source and the people/payroll lifecycle JOIN. Both previously
 * repeated this byte-identical statement; one literal now serves each so the
 * projection cannot drift. No lock, control-plane read or earlier helper is
 * changed. */
export const organizationGpoRulesProjection = 'SELECT "id","company_id","target_kind","target_id","version","revision","connection_id","gpo_id","user_filter_group_id","computer_target_ou_id","active","directory_config_status","directory_config_error","legacy_position_id","last_actor_id","last_reason","updated_at" FROM organization_gpo_rules';
