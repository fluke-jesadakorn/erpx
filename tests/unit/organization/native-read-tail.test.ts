import {describe,expect,it,vi} from 'vitest';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';

// The retired owner-local readers build no SQL of their own. Mock the native
// transport and assert the exact typed kind and positional values they send.
const one = vi.hoisted(() => vi.fn(async (_db: unknown, _companyId: string, _kind: string, _values: unknown[]) => null as unknown));
const all = vi.hoisted(() => vi.fn(async (_db: unknown, _companyId: string, _kind: string, _values: unknown[]) => [] as unknown[]));
vi.mock('../../../packages/server-organization/src/adapters/native-organization-owner-write', () => ({
  nativeOrgOne: one,
  nativeOrgAll: all,
}));
const {companyAcceptanceResultProjection} = await import('@erpx/server-organization/adapters/company-acceptance-result-projection');
const {activeDepartmentFact} = await import('@erpx/server-organization/adapters/profile-facts');
const {publicMarketingReceivingBranch} = await import('@erpx/server-organization/adapters/public-marketing-receiving-branch');
const {workReportOrganizationFacts} = await import('@erpx/server-organization/public/work-report-facts');
const {activeSpendingPosition} = await import('@erpx/server-organization/adapters/spending-position');
const {pmsRolloutOrganizationFacts} = await import('@erpx/server-organization/adapters/pms-rollout-facts');
const {calendarAccessOrganizationFacts} = await import('@erpx/server-organization/adapters/calendar-access-facts');

function db(): SqlDatabase {
  return {prepare: vi.fn(() => {throw new Error('owner-local reader must not build local SQL');})} as unknown as SqlDatabase;
}

describe('Organization owner-local read tail', () => {
  it('reads the acceptance result projection by typed kind with only positional values', async () => {
    all.mockClear();
    const rows = [{id: 'company-acceptance-result-key-a', company_id: 'company'}] as never[];
    all.mockResolvedValueOnce(rows);
    const handle = db();
    await expect(companyAcceptanceResultProjection(handle, 'company', 'key', 1, 2)).resolves.toBe(rows);
    expect(all).toHaveBeenCalledWith(handle, 'company', 'company_acceptance_result_projection', ['key', 1, 2, 'company']);
  });

  it('reduces the active-department fact from a typed single-row owner read', async () => {
    one.mockClear();
    const handle = db();
    one.mockResolvedValueOnce({one: 1} as never);
    await expect(activeDepartmentFact(handle, 'company', 'department')).resolves.toBe(true);
    one.mockResolvedValueOnce(null as never);
    await expect(activeDepartmentFact(handle, 'company', 'department')).resolves.toBe(false);
    expect(one).toHaveBeenCalledWith(handle, 'company', 'profile_active_department', ['department', 'company']);
  });

  it('resolves the public-form receiving branch by typed kind with only the Company value', async () => {
    one.mockClear();
    const handle = db();
    one.mockResolvedValueOnce({id: 'branch'} as never);
    await expect(publicMarketingReceivingBranch(handle, 'company')).resolves.toEqual({id: 'branch'});
    expect(one).toHaveBeenCalledWith(handle, 'company', 'public_marketing_receiving_branch', ['company']);
  });

  it('reads work-report nodes by typed kind and keeps the retained results shape', async () => {
    all.mockClear();
    const handle = db();
    const rows = [{id: 'node', display_name: 'Node'}] as never[];
    all.mockResolvedValueOnce(rows);
    await expect(workReportOrganizationFacts(handle).nodes('company')).resolves.toEqual({results: rows});
    expect(all).toHaveBeenCalledWith(handle, 'company', 'work_report_organization_nodes', ['company']);
  });

  it('reduces the active-spending-position fact from a typed single-row owner read', async () => {
    one.mockClear();
    const handle = db();
    one.mockResolvedValueOnce({one: 1} as never);
    await expect(activeSpendingPosition(handle, 'company', 'position')).resolves.toBe(true);
    one.mockResolvedValueOnce(null as never);
    await expect(activeSpendingPosition(handle, 'company', 'position')).resolves.toBe(false);
    expect(one).toHaveBeenCalledWith(handle, 'company', 'spending_active_position', ['position', 'company']);
  });

  it('reduces the PMS rollout active-branch fact from a typed single-row owner read', async () => {
    one.mockClear();
    const handle = db();
    const row = {'1': 1} as never;
    one.mockResolvedValueOnce(row);
    await expect(pmsRolloutOrganizationFacts(handle).organization('company')).resolves.toBe(row);
    one.mockResolvedValueOnce(null as never);
    await expect(pmsRolloutOrganizationFacts(handle).organization('company')).resolves.toBeNull();
    expect(one).toHaveBeenCalledWith(handle, 'company', 'pms_rollout_active_branch', ['company']);
  });

  it('reads calendar-access organization nodes by typed kind and keeps the row-list shape', async () => {
    all.mockClear();
    const handle = db();
    const rows = [{id: 'node', parentId: null, name: 'Node', kind: 'branch', branchId: null}] as never[];
    all.mockResolvedValueOnce(rows);
    await expect(calendarAccessOrganizationFacts(handle).organizationNodes('company')).resolves.toBe(rows);
    expect(all).toHaveBeenCalledWith(handle, 'company', 'calendar_access_organization_nodes', ['company']);
  });

  it('shares one canonical projection source across the owner projection maps', async () => {
    const {ORG_NODES_SOURCE,ORG_POSITIONS_SOURCE} = await import('../../../packages/server-organization/src/adapters/org-sources');
    const {commercialOwnerProjectionSources} = await import('@erpx/server-organization/adapters/commercial-owner-projection-sources');
    const {finalRootOwnerProjectionSources} = await import('@erpx/server-organization/adapters/final-root-owner-projection-sources');
    const {hospitalityOwnerProjectionSources} = await import('@erpx/server-organization/adapters/hospitality-owner-projection-sources');
    const {inventoryExecutiveOwnerProjections} = await import('@erpx/server-organization/adapters/inventory-executive-owner-projections');
    const {platformOwnerProjectionSources} = await import('@erpx/server-organization/adapters/platform-owner-projection-sources');
    for (const map of [commercialOwnerProjectionSources, finalRootOwnerProjectionSources, hospitalityOwnerProjectionSources, inventoryExecutiveOwnerProjections, platformOwnerProjectionSources])
      expect(map.organization_nodes).toBe(ORG_NODES_SOURCE);
    for (const map of [finalRootOwnerProjectionSources, hospitalityOwnerProjectionSources])
      expect(map.organization_positions).toBe(ORG_POSITIONS_SOURCE);
    expect(ORG_NODES_SOURCE).toContain('FROM organization_nodes');
    expect(ORG_POSITIONS_SOURCE).toContain('FROM organization_positions');
  });

  it('shares one canonical branch-Company source across the beta invitation readers', async () => {
    const {ORG_BRANCH_COMPANY_SOURCE} = await import('../../../packages/server-organization/src/adapters/org-sources');
    const {betaManagementOrganizationSources} = await import('@erpx/server-organization/adapters/beta-management-sources');
    const {betaInvitationSources} = await import('@erpx/server-organization/adapters/beta-invitation-sources');
    const seen: string[] = [];
    const first = vi.fn(async () => ({company_id: 'company'}));
    const bind = vi.fn(() => ({first}));
    const handle = {prepare: vi.fn((sql: string) => {seen.push(sql); return {bind};})} as unknown as SqlDatabase;
    await expect(betaManagementOrganizationSources(handle).branchCompany('branch')).resolves.toEqual({company_id: 'company'});
    await expect(betaInvitationSources(handle).invitationBranch('branch')).resolves.toEqual({company_id: 'company'});
    expect(seen).toEqual([ORG_BRANCH_COMPANY_SOURCE, ORG_BRANCH_COMPANY_SOURCE]);
    expect(ORG_BRANCH_COMPANY_SOURCE).toBe("SELECT company_id FROM organization_nodes WHERE id=? AND kind='branch' AND status='active'");
  });

  it('shares one canonical Company display-name source across the PMS/POS owner facts', async () => {
    const {ORG_NODE_DISPLAY_NAME_SOURCE} = await import('../../../packages/server-organization/src/adapters/org-sources');
    const {pmsHospitalitySetupOrganizationFacts} = await import('@erpx/server-organization/adapters/pms-hospitality-setup-facts');
    const {posCoreOrganizationFacts} = await import('@erpx/server-organization/adapters/pos-core-facts');
    const {PosReadOrganizationFacts} = await import('@erpx/server-organization/adapters/pos-facts');
    const seen: string[] = [];
    const first = vi.fn(async () => ({display_name: 'Acme'}));
    const bind = vi.fn(() => ({first}));
    const handle = {prepare: vi.fn((sql: string) => {seen.push(sql); return {bind};})} as unknown as SqlDatabase;
    await expect(pmsHospitalitySetupOrganizationFacts(handle).companyName({company: 'company'})).resolves.toEqual({display_name: 'Acme'});
    await expect(posCoreOrganizationFacts(handle).readOrganizationNodes2({company: 'company'})).resolves.toEqual({display_name: 'Acme'});
    await expect(PosReadOrganizationFacts(handle).readOrganizationNodes1({company: 'company'})).resolves.toEqual({display_name: 'Acme'});
    expect(seen).toEqual([ORG_NODE_DISPLAY_NAME_SOURCE, ORG_NODE_DISPLAY_NAME_SOURCE, ORG_NODE_DISPLAY_NAME_SOURCE]);
    expect(ORG_NODE_DISPLAY_NAME_SOURCE).toBe('SELECT display_name FROM organization_nodes WHERE id=?');
  });

  it('shares one canonical Company-scoped display-name source across the finance/payroll company reads', async () => {
    const {ORG_COMPANY_DISPLAY_NAME_SOURCE} = await import('../../../packages/server-organization/src/adapters/org-sources');
    const {financeSalesCompanyName} = await import('@erpx/server-organization/adapters/finance-sales-company');
    const {companySource} = await import('@erpx/server-organization/adapters/payroll-payslip-company');
    const seen: string[] = [];
    const first = vi.fn(async (column?: string) => column === 'display_name' ? 'Acme' : {display_name: 'Acme'});
    const bind = vi.fn(() => ({first}));
    const handle = {prepare: vi.fn((sql: string) => {seen.push(sql); return {bind};})} as unknown as SqlDatabase;
    await expect(financeSalesCompanyName(handle, 'company')).resolves.toBe('Acme');
    await expect(companySource(handle).company('company')).resolves.toEqual({display_name: 'Acme'});
    expect(seen).toEqual([ORG_COMPANY_DISPLAY_NAME_SOURCE, ORG_COMPANY_DISPLAY_NAME_SOURCE]);
    expect(ORG_COMPANY_DISPLAY_NAME_SOURCE).toBe("SELECT display_name FROM organization_nodes WHERE id=? AND kind='company'");
  });

  it('shares one canonical authority-seats projection fragment across the IAM and workflow consumers', async () => {
    const {identityVerifiedContextProjections} = await import('@erpx/server-organization/adapters/identity-access-verified-context-projections');
    const projections = identityVerifiedContextProjections();
    expect(projections.iam_store_authority_seats_s_from).toBe(projections.workflow_approvals_facts_authority_seats_s_from);
    expect(projections.iam_store_authority_seats_s_from).toBe('FROM (SELECT "company_id","kind","scope_id","version","position_id","person_id","effective_at" FROM authority_seats) s');
  });

  it('shares one canonical active-branch source across the collaboration home and beta readers', async () => {
    const {ORG_ACTIVE_BRANCHES_SOURCE} = await import('../../../packages/server-organization/src/adapters/org-sources');
    const {collaborationHomeSources} = await import('@erpx/server-organization/adapters/collaboration-home-sources');
    const {betaManagementFacts} = await import('@erpx/server-organization/adapters/beta-management-facts');
    const rows = [{id: 'branch', display_name: 'Branch'}] as never[];
    const rowsAll = vi.fn(async () => ({results: rows}));
    const bind = vi.fn(() => ({all: rowsAll}));
    const seen: string[] = [];
    const handle = {prepare: vi.fn((sql: string) => {seen.push(sql); return {bind};})} as unknown as SqlDatabase;
    await expect(collaborationHomeSources(handle).branches('company', 'branch', 'branch')).resolves.toEqual({results: rows});
    await expect(betaManagementFacts(handle).branches('company', 'branch', 'branch')).resolves.toEqual(rows);
    expect(seen).toEqual([ORG_ACTIVE_BRANCHES_SOURCE, ORG_ACTIVE_BRANCHES_SOURCE]);
    expect(ORG_ACTIVE_BRANCHES_SOURCE).toBe("SELECT id,display_name FROM organization_nodes WHERE kind='branch' AND company_id=? AND status='active' AND (?::text IS NULL OR id=?)");
  });

  it('shares one canonical roster-required source across the save-directory person reads', async () => {
    const {peopleDirectoryOrganizationFacts} = await import('@erpx/server-organization/adapters/people-directory-facts');
    const seen: string[] = [];
    const first = vi.fn(async () => ({roster_required: 1}));
    const bind = vi.fn(() => ({first}));
    const handle = {prepare: vi.fn((sql: string) => {seen.push(sql); return {bind};})} as unknown as SqlDatabase;
    const facts = peopleDirectoryOrganizationFacts(handle);
    await expect(facts.readSaveDirectoryPersonPrevious('company', 'position')).resolves.toEqual({roster_required: 1});
    await expect(facts.readSaveDirectoryPersonNext('company', 'position')).resolves.toEqual({roster_required: 1});
    expect(seen).toEqual([
      'SELECT roster_required FROM organization_positions WHERE company_id=? AND id=?',
      'SELECT roster_required FROM organization_positions WHERE company_id=? AND id=?',
    ]);
  });

  it('shares one canonical alias=organization_nodes projection across the IAM snapshot source map', async () => {
    const {identitySnapshotSources} = await import('../../../packages/server-organization/src/adapters/identity-snapshot-sources');
    const p = identitySnapshotSources();
    const keys = [
      'access_diagnosis_organization_nodes_from',
      'attachments_finance_facts_organization_nodes_from',
      'authorization_organization_nodes_from',
      'executive_main_evidence_sources_organization_nodes_from',
      'executive_metric_projections_organization_nodes_from',
      'iam_service_organization_nodes_from',
      'member_directory_organization_nodes_from',
      'user_management_organization_nodes_from',
      'workflow_role_facts_organization_nodes_from',
    ] as const;
    const expected = 'FROM (SELECT "id", "kind", "slug", "display_name", "parent_id", "company_id", "branch_id", "status", "hierarchy_revision" FROM organization_nodes) organization_nodes';
    for (const key of keys) expect(p[key]).toBe(expected);
  });

  it('shares one canonical alias=organization_nodes projection across the collaboration notification snapshot source map', async () => {
    const {collaborationNotificationSnapshotSources} = await import('../../../packages/server-organization/src/adapters/collaboration-notification-snapshot-sources');
    const p = collaborationNotificationSnapshotSources();
    const keys = [
      'organization_channels_organization_nodes_from',
      'organization_node_channels_organization_nodes_from',
      'people_roster_facts_organization_nodes_from',
    ] as const;
    const expected = 'FROM (SELECT "id", "kind", "slug", "display_name", "parent_id", "company_id", "branch_id", "status", "hierarchy_revision" FROM organization_nodes) organization_nodes';
    for (const key of keys) expect(p[key]).toBe(expected);
  });

  it('shares one canonical alias=organization_nodes projection across the IAM and collaboration snapshot source maps', async () => {
    const spaced = await import('../../../packages/server-organization/src/adapters/organization-nodes-spaced-projections');
    const {identitySnapshotSources} = await import('../../../packages/server-organization/src/adapters/identity-snapshot-sources');
    const {collaborationNotificationSnapshotSources} = await import('../../../packages/server-organization/src/adapters/collaboration-notification-snapshot-sources');
    const id = identitySnapshotSources();
    const cn = collaborationNotificationSnapshotSources();
    const keys = [
      'access_diagnosis_organization_nodes_from',
      'authorization_organization_nodes_from',
      'member_directory_organization_nodes_from',
      'user_management_organization_nodes_from',
      'workflow_role_facts_organization_nodes_from',
    ] as const;
    for (const key of keys) expect(id[key]).toBe(spaced.ORG_NODES_PROJECTION);
    for (const key of [
      'organization_channels_organization_nodes_from',
      'organization_node_channels_organization_nodes_from',
      'people_roster_facts_organization_nodes_from',
    ] as const) expect(cn[key]).toBe(spaced.ORG_NODES_PROJECTION);
    expect(spaced.ORG_NODES_PROJECTION).toBe('FROM (SELECT "id", "kind", "slug", "display_name", "parent_id", "company_id", "branch_id", "status", "hierarchy_revision" FROM organization_nodes) organization_nodes');
  });

  it('shares one canonical alias=l staff_levels projection across the people/payroll final-owner source map', async () => {
    const {peoplePayrollFinalOwnerProjections} = await import('../../../packages/server-organization/src/adapters/people-payroll-final-owner-projections');
    const p = peoplePayrollFinalOwnerProjections();
    const keys = [
      'executive_engagement_source_staff_levels_l_join',
      'executive_report_sources_staff_levels_l_join',
      'organization_roster_facts_staff_levels_l_join',
      'workflow_approvals_facts_staff_levels_l_join',
    ] as const;
    const expected = 'JOIN (SELECT "id","company_id","code","name","rank","active","version","created_at","updated_at","level_number" FROM staff_levels) l';
    for (const key of keys) expect(p[key]).toBe(expected);
  });

  it('shares canonical alias=n/c organization_nodes and alias=op organization_positions joins across the people/payroll final-owner source map', async () => {
    const {peoplePayrollFinalOwnerProjections} = await import('../../../packages/server-organization/src/adapters/people-payroll-final-owner-projections');
    const p = peoplePayrollFinalOwnerProjections();
    const nodesN = [
      'executive_report_sources_organization_nodes_n_join',
      'people_evaluation_organization_nodes_n_join',
      'workflow_flow_routing_facts_organization_nodes_n_join',
    ] as const;
    const nodesC = [
      'executive_report_sources_organization_nodes_c_join',
      'profile_work_organization_nodes_c_join',
      'people_directory_repository_organization_nodes_c_join',
    ] as const;
    const positionsOp = [
      'profile_work_organization_positions_op_join',
      'people_directory_repository_organization_positions_op_join',
    ] as const;
    const nodesExpected = 'JOIN (SELECT "id","kind","slug","display_name","parent_id","company_id","branch_id","status","hierarchy_revision" FROM organization_nodes)';
    const firstN = p[nodesN[0]];
    const firstC = p[nodesC[0]];
    const firstOp = p[positionsOp[0]];
    for (const key of nodesN) expect(p[key]).toBe(firstN);
    for (const key of nodesC) expect(p[key]).toBe(firstC);
    for (const key of positionsOp) expect(p[key]).toBe(firstOp);
    expect(firstN).toBe(`${nodesExpected} n`);
    expect(firstC).toBe(`${nodesExpected} c`);
    expect(firstOp).toBe('JOIN (SELECT "id","company_id","branch_id","department_id","title","level_id","reports_to_position_id","is_ceo","active","version","created_by","created_at","updated_at","requirements","roster_required" FROM organization_positions) op');
  });

  it('shares one canonical spaced-column projection per alias across the IAM snapshot source maps', async () => {
    const c = await import('../../../packages/server-organization/src/adapters/organization-nodes-spaced-projections');
    const {identitySnapshotSources} = await import('../../../packages/server-organization/src/adapters/identity-snapshot-sources');
    const {learningTalentSnapshotSources} = await import('../../../packages/server-organization/src/adapters/learning-talent-snapshot-sources');
    const {collaborationNotificationSnapshotSources} = await import('../../../packages/server-organization/src/adapters/collaboration-notification-snapshot-sources');
    const id = identitySnapshotSources();
    const lt = learningTalentSnapshotSources();
    const cn = collaborationNotificationSnapshotSources();

    const nFrom = [
      'access_diagnosis_organization_nodes_n_from',
      'authorization_organization_nodes_n_from',
      'executive_authority_source_organization_nodes_n_from',
      'executive_metric_projections_organization_nodes_n_from',
      'executive_report_sources_organization_nodes_n_from',
      'user_management_organization_nodes_n_from',
      'work_role_selection_facts_organization_nodes_n_from',
      'workflow_command_authority_organization_nodes_n_from',
    ] as const;
    for (const key of nFrom) expect(id[key]).toBe(c.ORG_NODES_SPACED_N_FROM);
    expect(lt.learning_adapters_executive_metric_projections_organization_nodes_n_from).toBe(c.ORG_NODES_SPACED_N_FROM);
    expect(lt.talent_adapters_executive_metric_projections_organization_nodes_n_from).toBe(c.ORG_NODES_SPACED_N_FROM);
    expect(cn.organization_channels_organization_nodes_n_from).toBe(c.ORG_NODES_SPACED_N_FROM);

    const groups: Array<[Record<string,string>, readonly string[], string]> = [
      [id, ['authorization_organization_nodes_c_join','directory_projection_facts_organization_nodes_c_join','executive_authority_source_organization_nodes_c_join','executive_connection_facts_organization_nodes_c_join','executive_report_sources_organization_nodes_c_join','organization_change_assignment_organization_nodes_c_join','profile_membership_organization_nodes_c_join'], c.ORG_NODES_SPACED_C_JOIN],
      [id, ['access_diagnosis_organization_nodes_b_from','authorization_organization_nodes_b_from','executive_authority_source_organization_nodes_b_from','executive_report_sources_organization_nodes_b_from','work_role_selection_facts_organization_nodes_b_from','workflow_command_authority_organization_nodes_b_from'], c.ORG_NODES_SPACED_B_FROM],
      [id, ['authorization_organization_nodes_n_join','executive_main_evidence_sources_organization_nodes_n_join','executive_report_sources_organization_nodes_n_join','finance_delegation_organization_nodes_n_join','gpo_rules_organization_nodes_n_join','iam_my_access_organization_nodes_n_join'], c.ORG_NODES_SPACED_N_JOIN],
      [id, ['authorization_organization_nodes_b_join','executive_main_evidence_sources_organization_nodes_b_join','executive_metric_projections_organization_nodes_b_join','executive_report_sources_organization_nodes_b_join','iam_api_organization_nodes_b_join'], c.ORG_NODES_SPACED_B_JOIN],
    ];
    for (const [map, keys, value] of groups) for (const key of keys) expect(map[key]).toBe(value);

    expect(c.ORG_NODES_SPACED_N_FROM).toBe('FROM (SELECT "id", "kind", "slug", "display_name", "parent_id", "company_id", "branch_id", "status", "hierarchy_revision" FROM organization_nodes) n');
    expect(c.ORG_NODES_SPACED_C_JOIN).toBe('JOIN (SELECT "id", "kind", "slug", "display_name", "parent_id", "company_id", "branch_id", "status", "hierarchy_revision" FROM organization_nodes) c');
    expect(c.ORG_NODES_SPACED_B_FROM).toBe('FROM (SELECT "id", "kind", "slug", "display_name", "parent_id", "company_id", "branch_id", "status", "hierarchy_revision" FROM organization_nodes) b');
    expect(c.ORG_NODES_SPACED_N_JOIN).toBe('JOIN (SELECT "id", "kind", "slug", "display_name", "parent_id", "company_id", "branch_id", "status", "hierarchy_revision" FROM organization_nodes) n');
    expect(c.ORG_NODES_SPACED_B_JOIN).toBe('JOIN (SELECT "id", "kind", "slug", "display_name", "parent_id", "company_id", "branch_id", "status", "hierarchy_revision" FROM organization_nodes) b');
  });

  it('shares the C_FROM projection, staff-levels join and branch-active guard literals', async () => {
    const spaced = await import('../../../packages/server-organization/src/adapters/organization-nodes-spaced-projections');
    const shared = await import('../../../packages/server-organization/src/adapters/organization-read-tail-sources');
    const {identitySnapshotSources} = await import('../../../packages/server-organization/src/adapters/identity-snapshot-sources');
    const id = identitySnapshotSources();

    const cFrom = [
      'payroll_cancellation_grant_organization_nodes_c_from',
      'work_role_selection_facts_organization_nodes_c_from',
      'workflow_command_authority_organization_nodes_c_from',
    ] as const;
    for (const key of cFrom) expect(id[key]).toBe(spaced.ORG_NODES_SPACED_C_FROM);

    const lJoin = [
      'executive_report_sources_staff_levels_l_join',
      'gpo_rules_staff_levels_l_join',
      'operations_staff_policy_facts_staff_levels_l_join',
    ] as const;
    for (const key of lJoin) expect(id[key]).toBe(shared.STAFF_LEVELS_L_JOIN);

    expect(spaced.ORG_NODES_SPACED_C_FROM).toBe('FROM (SELECT "id", "kind", "slug", "display_name", "parent_id", "company_id", "branch_id", "status", "hierarchy_revision" FROM organization_nodes) c');
    expect(shared.STAFF_LEVELS_L_JOIN).toBe('JOIN (SELECT "id", "company_id", "code", "name", "rank", "active", "version", "created_at", "updated_at", "level_number" FROM staff_levels) l');
    expect(shared.NODE_BRANCH_ACTIVE_EXISTS).toBe("EXISTS(SELECT 1 FROM organization_nodes WHERE id=? AND company_id=? AND kind='branch' AND status='active')");
    expect(shared.NODE_BRANCH_ACTIVE).toBe("SELECT 1 FROM organization_nodes WHERE id=? AND company_id=? AND kind='branch' AND status='active'");
  });

  it('shares one table-parameterized active-id read across the evaluation and placement readers', async () => {
    const {activeIdByCompanySource} = await import('../../../packages/server-organization/src/adapters/org-sources');
    const {peopleEvaluationOrganizationFacts} = await import('@erpx/server-organization/adapters/people-evaluation-facts');
    const {peoplePlacementBatchFacts} = await import('@erpx/server-organization/adapters/people-placement-batch-facts');
    const seen: string[] = [];
    const rowsAll = vi.fn(async () => ({results: [] as unknown[]}));
    const bind = vi.fn(() => ({all: rowsAll}));
    const handle = {prepare: vi.fn((sql: string) => {seen.push(sql); return {bind};})} as unknown as SqlDatabase;
    await expect(peopleEvaluationOrganizationFacts(handle).readImportEvaluationMarketLevels('company')).resolves.toEqual({results: []});
    await expect(peoplePlacementBatchFacts(handle).activePositions('company')).resolves.toEqual({results: []});
    expect(seen).toEqual([
      activeIdByCompanySource('staff_levels'),
      activeIdByCompanySource('organization_positions'),
    ]);
    expect(activeIdByCompanySource('staff_levels')).toBe('SELECT id FROM staff_levels WHERE company_id=? AND active=1');
    expect(activeIdByCompanySource('organization_positions')).toBe('SELECT id FROM organization_positions WHERE company_id=? AND active=1');
  });

  it('renders one byte-identical table-parameterized department absence guard', async () => {
    const {departmentSnapshotMissing} = await import('../../../packages/server-organization/src/adapters/organization-read-tail-sources');
    expect(departmentSnapshotMissing('department_heads')).toBe('NOT EXISTS(SELECT 1 FROM department_heads WHERE company_id=? AND department_id=?)');
    expect(departmentSnapshotMissing('department_flow_versions')).toBe('NOT EXISTS(SELECT 1 FROM department_flow_versions WHERE company_id=? AND department_id=?)');
    expect(departmentSnapshotMissing('department_heads').replace('department_heads', 'T')).toBe(
      departmentSnapshotMissing('department_flow_versions').replace('department_flow_versions', 'T'));
  });

  it('renders one byte-identical table-parameterized attachment version guard', async () => {
    const {ownerVersionGuardSource} = await import('../../../packages/server-organization/src/adapters/organization-read-tail-sources');
    // The Finance and Service attachment facts previously inlined the same
    // guard; only the relation varied. The shared helper renders it unchanged.
    expect(ownerVersionGuardSource('organization_node_changes')).toBe(
      'EXISTS(SELECT 1 FROM organization_node_changes WHERE id=? AND company_id=? AND version=?)');
    expect(ownerVersionGuardSource('organization_node_changes', 'version')).toBe(
      'EXISTS(SELECT 1 FROM organization_node_changes WHERE id=? AND company_id=? AND version=?)');
    expect(ownerVersionGuardSource('t', 'version', 'active=1')).toBe(
      'EXISTS(SELECT 1 FROM t WHERE id=? AND company_id=? AND version=? AND active=1)');
    expect(ownerVersionGuardSource('organization_node_changes').replace('organization_node_changes', 'T')).toBe(
      ownerVersionGuardSource('t').replace('t', 'T'));
  });

  it('renders one byte-identical alias-parameterized level_leave_defaults projection', async () => {
    const {peopleVerifiedContextProjections} = await import('../../../packages/server-organization/src/adapters/people-payroll-verified-context-projections');
    const p = peopleVerifiedContextProjections();
    const d = p.people_payroll_level_leave_defaults_d_from;
    const x = p.people_payroll_level_leave_defaults_x_from;
    const cols = '"id","company_id","level_id","version","policy_version_id","created_by","created_at"';
    expect(d).toBe(`FROM (SELECT ${cols} FROM level_leave_defaults) d`);
    expect(x).toBe(`FROM (SELECT ${cols} FROM level_leave_defaults) x`);
    expect(d.replace(' d', ' A')).toBe(x.replace(' x', ' A'));
    expect(p.people_payroll_position_pay_defaults_from).toContain('FROM position_pay_defaults) position_pay_defaults');
  });

  it('renders byte-identical alias/keyword-parameterized final-owner projections', async () => {
    const {peoplePayrollFinalOwnerProjections} = await import('../../../packages/server-organization/src/adapters/people-payroll-final-owner-projections');
    const p = peoplePayrollFinalOwnerProjections();
    const nodeCols = '"id","kind","slug","display_name","parent_id","company_id","branch_id","status","hierarchy_revision"';
    const posCols = '"id","company_id","branch_id","department_id","title","level_id","reports_to_position_id","is_ceo","active","version","created_by","created_at","updated_at","requirements","roster_required"';
    const staffCols = '"id","company_id","code","name","rank","active","version","created_at","updated_at","level_number"';
    const strip = (s: string) => s.replace(/^(FROM|JOIN) /, '');
    const bJoin = p.executive_report_sources_organization_nodes_b_join;
    const nFrom = p.operations_staff_settings_facts_organization_nodes_n_from;
    expect(bJoin).toBe(`JOIN (SELECT ${nodeCols} FROM organization_nodes) b`);
    expect(nFrom).toBe(`FROM (SELECT ${nodeCols} FROM organization_nodes) n`);
    expect(strip(bJoin).replace(' b', ' A')).toBe(strip(nFrom).replace(' n', ' A'));
    const posFrom = p.operations_staff_settings_facts_organization_positions_from;
    const pFrom = p.organization_authority_people_organization_positions_p_from;
    expect(posFrom).toBe(`FROM (SELECT ${posCols} FROM organization_positions) organization_positions`);
    expect(pFrom).toBe(`FROM (SELECT ${posCols} FROM organization_positions) p`);
    expect(strip(posFrom).replace(') organization_positions', ') A')).toBe(strip(pFrom).replace(') p', ') A'));
    const staffFrom = p.executive_metric_projections_staff_levels_from;
    expect(staffFrom).toBe(`FROM (SELECT ${staffCols} FROM staff_levels) staff_levels`);
    expect(strip(staffFrom).replace(') staff_levels', ') l')).toBe(
      strip(p.executive_engagement_source_staff_levels_l_join));
  });

});
