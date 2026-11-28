import {createHash} from 'node:crypto';
import {readdirSync,readFileSync} from 'node:fs';
import {resolve} from 'node:path';
import {describe,expect,it,vi} from 'vitest';
import type {SqlDatabase} from '@erpx/platform-runtime-server/public/runtime/types';

// The retired owner-local readers build no SQL of their own. Mock the native
// transport and assert the exact typed kind and positional values they send.
const one = vi.hoisted(() => vi.fn(async (_db: unknown, _companyId: string, _kind: string, _values: unknown[]) => null as unknown));
const all = vi.hoisted(() => vi.fn(async (_db: unknown, _companyId: string, _kind: string, _values: unknown[]) => [] as unknown[]));
const execute = vi.hoisted(() => vi.fn(async (_companyId: string, _kind: string, _values: unknown[], _key: string) => ({kind: _kind, rows: 0})));
const laneMarketLevels = vi.hoisted(() => vi.fn(async (_target: unknown, _companyId: string) => [] as unknown[]));
vi.mock('../../../modules/organization/server/src/adapters/native-organization-owner-write', () => ({
  nativeOrgOne: one,
  nativeOrgAll: all,
  nativeOrgExecute: execute,
}));
vi.mock('../../../modules/organization/server/src/adapters/native-lane-24/client', () => ({
  nativeLane24MarketLevels: laneMarketLevels,
}));
const {companyAcceptanceResultProjection} = await import('@erpx/organization-server/adapters/company-acceptance-result-projection');
const {activeDepartmentFact} = await import('@erpx/organization-server/adapters/profile-facts');
const {publicMarketingReceivingBranch} = await import('@erpx/organization-server/adapters/public-marketing-receiving-branch');
const {workReportOrganizationFacts} = await import('@erpx/organization-server/public/work-report-facts');
const {activeSpendingPosition} = await import('@erpx/organization-server/adapters/spending-position');
const {pmsRolloutOrganizationFacts} = await import('@erpx/organization-server/adapters/pms-rollout-facts');
const {calendarAccessOrganizationFacts} = await import('@erpx/organization-server/adapters/calendar-access-facts');

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
    const {ORG_NODES_SOURCE,ORG_POSITIONS_SOURCE} = await import('../../../modules/organization/server/src/adapters/org-sources');
    const {commercialOwnerProjectionSources} = await import('@erpx/organization-server/adapters/commercial-owner-projection-sources');
    const {finalRootOwnerProjectionSources} = await import('@erpx/organization-server/adapters/final-root-owner-projection-sources');
    const {hospitalityOwnerProjectionSources} = await import('@erpx/organization-server/adapters/hospitality-owner-projection-sources');
    const {inventoryExecutiveOwnerProjections} = await import('@erpx/organization-server/adapters/inventory-executive-owner-projections');
    const {platformOwnerProjectionSources} = await import('@erpx/organization-server/adapters/platform-owner-projection-sources');
    for (const map of [commercialOwnerProjectionSources, finalRootOwnerProjectionSources, hospitalityOwnerProjectionSources, inventoryExecutiveOwnerProjections, platformOwnerProjectionSources])
      expect(map.organization_nodes).toBe(ORG_NODES_SOURCE);
    for (const map of [finalRootOwnerProjectionSources, hospitalityOwnerProjectionSources])
      expect(map.organization_positions).toBe(ORG_POSITIONS_SOURCE);
    expect(ORG_NODES_SOURCE).toContain('FROM organization_nodes');
    expect(ORG_POSITIONS_SOURCE).toContain('FROM organization_positions');
  });

  it('shares one canonical branch-Company source across the beta invitation readers', async () => {
    const {ORG_BRANCH_COMPANY_SOURCE} = await import('../../../modules/organization/server/src/adapters/org-sources');
    const {betaManagementOrganizationSources} = await import('@erpx/organization-server/adapters/beta-management-sources');
    const {betaInvitationSources} = await import('@erpx/organization-server/adapters/beta-invitation-sources');
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
    const {ORG_NODE_DISPLAY_NAME_SOURCE} = await import('../../../modules/organization/server/src/adapters/org-sources');
    const {pmsHospitalitySetupOrganizationFacts} = await import('@erpx/organization-server/adapters/pms-hospitality-setup-facts');
    const {posCoreOrganizationFacts} = await import('@erpx/organization-server/adapters/pos-core-facts');
    const {PosReadOrganizationFacts} = await import('@erpx/organization-server/adapters/pos-facts');
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
    const {ORG_COMPANY_DISPLAY_NAME_SOURCE} = await import('../../../modules/organization/server/src/adapters/org-sources');
    const {financeSalesCompanyName} = await import('@erpx/organization-server/adapters/finance-sales-company');
    const {companySource} = await import('@erpx/organization-server/adapters/payroll-payslip-company');
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
    const {identityVerifiedContextProjections} = await import('@erpx/organization-server/adapters/identity-access-verified-context-projections');
    const projections = identityVerifiedContextProjections();
    expect(projections.iam_store_authority_seats_s_from).toBe(projections.workflow_approvals_facts_authority_seats_s_from);
    expect(projections.iam_store_authority_seats_s_from).toBe('FROM (SELECT "company_id","kind","scope_id","version","position_id","person_id","effective_at" FROM authority_seats) s');
  });

  it('shares one canonical active-branch source across the collaboration home and beta readers', async () => {
    const {ORG_ACTIVE_BRANCHES_SOURCE} = await import('../../../modules/organization/server/src/adapters/org-sources');
    const {collaborationHomeSources} = await import('@erpx/organization-server/adapters/collaboration-home-sources');
    const {betaManagementFacts} = await import('@erpx/organization-server/adapters/beta-management-facts');
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
    const {peopleDirectoryOrganizationFacts} = await import('@erpx/organization-server/adapters/people-directory-facts');
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
    const {identitySnapshotSources} = await import('../../../modules/organization/server/src/adapters/identity-snapshot-sources');
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
    const {collaborationNotificationSnapshotSources} = await import('../../../modules/organization/server/src/adapters/collaboration-notification-snapshot-sources');
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
    const spaced = await import('../../../modules/organization/server/src/adapters/organization-nodes-spaced-projections');
    const {identitySnapshotSources} = await import('../../../modules/organization/server/src/adapters/identity-snapshot-sources');
    const {collaborationNotificationSnapshotSources} = await import('../../../modules/organization/server/src/adapters/collaboration-notification-snapshot-sources');
    const id = identitySnapshotSources();
    const cn = collaborationNotificationSnapshotSources();
    const projection = spaced.ORG_NODES_SPACED_CLAUSE('FROM','organization_nodes');
    const keys = [
      'access_diagnosis_organization_nodes_from',
      'authorization_organization_nodes_from',
      'member_directory_organization_nodes_from',
      'user_management_organization_nodes_from',
      'workflow_role_facts_organization_nodes_from',
    ] as const;
    for (const key of keys) expect(id[key]).toBe(projection);
    for (const key of [
      'organization_channels_organization_nodes_from',
      'organization_node_channels_organization_nodes_from',
      'people_roster_facts_organization_nodes_from',
    ] as const) expect(cn[key]).toBe(projection);
    expect(projection).toBe('FROM (SELECT "id", "kind", "slug", "display_name", "parent_id", "company_id", "branch_id", "status", "hierarchy_revision" FROM organization_nodes) organization_nodes');
  });

  it('shares one canonical alias=l staff_levels projection across the people/payroll final-owner source map', async () => {
    const {peoplePayrollFinalOwnerProjections} = await import('../../../modules/organization/server/src/adapters/people-payroll-final-owner-projections');
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
    const {peoplePayrollFinalOwnerProjections} = await import('../../../modules/organization/server/src/adapters/people-payroll-final-owner-projections');
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
    const c = await import('../../../modules/organization/server/src/adapters/organization-nodes-spaced-projections');
    const {identitySnapshotSources} = await import('../../../modules/organization/server/src/adapters/identity-snapshot-sources');
    const {learningTalentSnapshotSources} = await import('../../../modules/organization/server/src/adapters/learning-talent-snapshot-sources');
    const {collaborationNotificationSnapshotSources} = await import('../../../modules/organization/server/src/adapters/collaboration-notification-snapshot-sources');
    const id = identitySnapshotSources();
    const lt = learningTalentSnapshotSources();
    const cn = collaborationNotificationSnapshotSources();
    const nFromClause = c.ORG_NODES_SPACED_CLAUSE('FROM','n');
    const bFromClause = c.ORG_NODES_SPACED_CLAUSE('FROM','b');

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
    for (const key of nFrom) expect(id[key]).toBe(nFromClause);
    expect(lt.learning_adapters_executive_metric_projections_organization_nodes_n_from).toBe(nFromClause);
    expect(lt.talent_adapters_executive_metric_projections_organization_nodes_n_from).toBe(nFromClause);
    expect(cn.organization_channels_organization_nodes_n_from).toBe(nFromClause);

    const groups: Array<[Record<string,string>, readonly string[], string]> = [
      [id, ['authorization_organization_nodes_c_join','directory_projection_facts_organization_nodes_c_join','executive_authority_source_organization_nodes_c_join','executive_connection_facts_organization_nodes_c_join','executive_report_sources_organization_nodes_c_join','organization_change_assignment_organization_nodes_c_join','profile_membership_organization_nodes_c_join'], c.ORG_NODES_SPACED_C_JOIN],
      [id, ['access_diagnosis_organization_nodes_b_from','authorization_organization_nodes_b_from','executive_authority_source_organization_nodes_b_from','executive_report_sources_organization_nodes_b_from','work_role_selection_facts_organization_nodes_b_from','workflow_command_authority_organization_nodes_b_from'], bFromClause],
      [id, ['authorization_organization_nodes_n_join','executive_main_evidence_sources_organization_nodes_n_join','executive_report_sources_organization_nodes_n_join','finance_delegation_organization_nodes_n_join','gpo_rules_organization_nodes_n_join','iam_my_access_organization_nodes_n_join'], c.ORG_NODES_SPACED_N_JOIN],
      [id, ['authorization_organization_nodes_b_join','executive_main_evidence_sources_organization_nodes_b_join','executive_metric_projections_organization_nodes_b_join','executive_report_sources_organization_nodes_b_join','iam_api_organization_nodes_b_join'], c.ORG_NODES_SPACED_B_JOIN],
    ];
    for (const [map, keys, value] of groups) for (const key of keys) expect(map[key]).toBe(value);

    expect(nFromClause).toBe('FROM (SELECT "id", "kind", "slug", "display_name", "parent_id", "company_id", "branch_id", "status", "hierarchy_revision" FROM organization_nodes) n');
    expect(c.ORG_NODES_SPACED_C_JOIN).toBe('JOIN (SELECT "id", "kind", "slug", "display_name", "parent_id", "company_id", "branch_id", "status", "hierarchy_revision" FROM organization_nodes) c');
    expect(bFromClause).toBe('FROM (SELECT "id", "kind", "slug", "display_name", "parent_id", "company_id", "branch_id", "status", "hierarchy_revision" FROM organization_nodes) b');
    expect(c.ORG_NODES_SPACED_N_JOIN).toBe('JOIN (SELECT "id", "kind", "slug", "display_name", "parent_id", "company_id", "branch_id", "status", "hierarchy_revision" FROM organization_nodes) n');
    expect(c.ORG_NODES_SPACED_B_JOIN).toBe('JOIN (SELECT "id", "kind", "slug", "display_name", "parent_id", "company_id", "branch_id", "status", "hierarchy_revision" FROM organization_nodes) b');
  });

  it('shares the C_FROM projection, staff-levels join and branch-active guard literals', async () => {
    const spaced = await import('../../../modules/organization/server/src/adapters/organization-nodes-spaced-projections');
    const shared = await import('../../../modules/organization/server/src/adapters/organization-read-tail-sources');
    const {identitySnapshotSources} = await import('../../../modules/organization/server/src/adapters/identity-snapshot-sources');
    const id = identitySnapshotSources();
    const cFromClause = spaced.ORG_NODES_SPACED_CLAUSE('FROM','c');

    const cFrom = [
      'payroll_cancellation_grant_organization_nodes_c_from',
      'work_role_selection_facts_organization_nodes_c_from',
      'workflow_command_authority_organization_nodes_c_from',
    ] as const;
    for (const key of cFrom) expect(id[key]).toBe(cFromClause);

    const lJoin = [
      'executive_report_sources_staff_levels_l_join',
      'gpo_rules_staff_levels_l_join',
      'operations_staff_policy_facts_staff_levels_l_join',
    ] as const;
    for (const key of lJoin) expect(id[key]).toBe(shared.STAFF_LEVELS_L_JOIN);

    expect(cFromClause).toBe('FROM (SELECT "id", "kind", "slug", "display_name", "parent_id", "company_id", "branch_id", "status", "hierarchy_revision" FROM organization_nodes) c');
    expect(shared.STAFF_LEVELS_L_JOIN).toBe('JOIN (SELECT "id", "company_id", "code", "name", "rank", "active", "version", "created_at", "updated_at", "level_number" FROM staff_levels) l');
    expect(shared.NODE_BRANCH_ACTIVE_EXISTS).toBe("EXISTS(SELECT 1 FROM organization_nodes WHERE id=? AND company_id=? AND kind='branch' AND status='active')");
    expect(shared.NODE_BRANCH_ACTIVE).toBe("SELECT 1 FROM organization_nodes WHERE id=? AND company_id=? AND kind='branch' AND status='active'");
  });

  it('delegates the evaluation active-id read to the native lane-24 client, placement stays owner-local', async () => {
    const {activeIdByCompanySource} = await import('../../../modules/organization/server/src/adapters/org-sources');
    const {peopleEvaluationOrganizationFacts} = await import('@erpx/organization-server/adapters/people-evaluation-facts');
    const {peoplePlacementBatchFacts} = await import('@erpx/organization-server/adapters/people-placement-batch-facts');
    const actor = {holdingId: 'h001', holdingSlug: 'acme', companyId: 'company'};
    laneMarketLevels.mockClear();
    const seen: string[] = [];
    const rowsAll = vi.fn(async () => ({results: [] as unknown[]}));
    const bind = vi.fn(() => ({all: rowsAll}));
    const handle = {prepare: vi.fn((sql: string) => {seen.push(sql); return {bind};})} as unknown as SqlDatabase;
    await expect(peopleEvaluationOrganizationFacts(handle, actor).readImportEvaluationMarketLevels('company')).resolves.toEqual({
      results: [], success: true, meta: {changes: 0, duration: 0},
    });
    expect(laneMarketLevels).toHaveBeenCalledWith(actor, 'company');
    expect(seen).toEqual([]);
    await expect(peoplePlacementBatchFacts(handle).activePositions('company')).resolves.toEqual({results: []});
    expect(seen).toEqual([activeIdByCompanySource('organization_positions')]);
    expect(activeIdByCompanySource('staff_levels')).toBe('SELECT id FROM staff_levels WHERE company_id=? AND active=1');
    expect(activeIdByCompanySource('organization_positions')).toBe('SELECT id FROM organization_positions WHERE company_id=? AND active=1');
  });

  it('renders one byte-identical table-parameterized department absence guard', async () => {
    const {departmentSnapshotMissing} = await import('../../../modules/organization/server/src/adapters/organization-read-tail-sources');
    expect(departmentSnapshotMissing('department_heads')).toBe('NOT EXISTS(SELECT 1 FROM department_heads WHERE company_id=? AND department_id=?)');
    expect(departmentSnapshotMissing('department_flow_versions')).toBe('NOT EXISTS(SELECT 1 FROM department_flow_versions WHERE company_id=? AND department_id=?)');
    expect(departmentSnapshotMissing('department_heads').replace('department_heads', 'T')).toBe(
      departmentSnapshotMissing('department_flow_versions').replace('department_flow_versions', 'T'));
  });

  it('renders one byte-identical table-parameterized attachment version guard', async () => {
    const {ownerVersionGuardSource} = await import('../../../modules/organization/server/src/adapters/organization-read-tail-sources');
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
    const {peopleVerifiedContextProjections} = await import('../../../modules/organization/server/src/adapters/people-payroll-verified-context-projections');
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
    const {peoplePayrollFinalOwnerProjections} = await import('../../../modules/organization/server/src/adapters/people-payroll-final-owner-projections');
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

describe('Organization read-tail r30 native cutover — finance starter node list', () => {
  const srcRoot = resolve(__dirname, '../../../modules/organization/server/src');
  const read = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
  const sha = (value: string) => createHash('sha256').update(value).digest('hex');
  const KIND = 'finance_starter_organization_nodes';
  const SQL = "SELECT id,kind,display_name FROM organization_nodes WHERE company_id=? AND kind IN ('branch','department') AND status='active' ORDER BY kind,id";
  const CALL = `nativeOrgAll<{id:string;kind:string;display_name:string}>(db,companyId,'${KIND}',[companyId])`;

  it('deletes the TS literal and sends only the typed kind and Company bind', async () => {
    const source = read('finance-starter-sources.ts');
    expect(source).not.toContain(SQL);
    expect(source).toContain(CALL);
    expect(source).toContain("import {nativeOrgAll} from './native-organization-owner-write';");
    expect(source.split('nativeOrgAll<').length - 1).toBe(1);
    expect(source.split(KIND).length - 1).toBe(1);
    // The Company-owning row lock is deliberately untouched by this slice.
    expect(source).toContain("kind='company' AND status='active' FOR UPDATE");
    const {financeStarterOrganizationNodes, lockFinanceStarterCompany} = await import(
      '../../../modules/organization/server/src/adapters/finance-starter-sources'
    );
    expect(lockFinanceStarterCompany).toBeTypeOf('function');
    all.mockClear();
    const db = {prepare: vi.fn(() => {throw new Error('TS read tail must not build SQL');})} as unknown as SqlDatabase;
    const rows = [
      {id: 'n1', kind: 'branch', display_name: 'New York'},
      {id: 'n2', kind: 'department', display_name: 'Finance'},
    ];
    all.mockResolvedValueOnce(rows);
    await expect(financeStarterOrganizationNodes(db, 'company-1')).resolves.toBe(rows);
    expect(all).toHaveBeenCalledWith(db, 'company-1', KIND, ['company-1']);
    expect(db.prepare).not.toHaveBeenCalled();
  });

  it('pins the byte-identical SQL now owned by the Rust registry', () => {
    expect(sha(SQL)).toBe('884e6239956ea25739936acc37fdee49c341f7dfa062478905c548507826a501');
    const registry = read('organization_owner_write.rs');
    expect(registry).toContain(`pub const FINANCE_STARTER_ORGANIZATION_NODES: &str = "${KIND}";`);
    expect(registry).toContain(`    FINANCE_STARTER_ORGANIZATION_NODES,\n];`);
    expect(registry).toContain(`        FINANCE_STARTER_ORGANIZATION_NODES => {\n            "${SQL}"\n        }`);
    expect(registry).not.toMatch(/FINANCE_STARTER_ORGANIZATION_NODES => \{[\s\S]{0,200}(INSERT|UPDATE|DELETE|FOR UPDATE|LOCK TABLE)/);
  });

  it('retirement:ts-statements records the cumulative native cutover', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, {withFileTypes: true})) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBe(178);
    expect(total).toBeLessThan(189);
    // The worker-position cut deleted the shared r38 helper literal after both
    // former consumers moved to the native owner, so the KPI falls by one.
    expect(read('people-lifecycle-facts.ts').split('SELECT ').length - 1).toBe(4);
  });
});

describe('Organization people-lifecycle node read native cutover', () => {
  const srcRoot = resolve(__dirname, '../../../modules/organization/server/src');
  const read = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
  const sha = (value: string) => createHash('sha256').update(value).digest('hex');
  const KIND = 'people_lifecycle_data_nodes';
  const SQL = "SELECT id,display_name name,kind FROM organization_nodes WHERE company_id=? AND status='active' ORDER BY display_name,id";
  const CALL = `nativeOrgAll<{`;

  it('deletes the TS literal and sends only the typed kind and Company bind', async () => {
    const source = read('people-lifecycle-facts.ts');
    expect(source).not.toContain(SQL);
    expect(source).toContain(CALL);
    expect(source).toContain(`}>(database, companyId, '${KIND}', [companyId]).then(results => ({ results }));`);
    expect(source).toContain("import { nativeOrgAll, nativeOrgExecute, nativeOrgOne } from './native-organization-owner-write';");
    expect(source.split(KIND).length - 1).toBe(1);
    // The retained Company-owning row lock and package effects stay untouched.
    expect(source).toContain('FOR UPDATE');
    const {peopleLifecycleOrganizationFacts} = await import(
      '../../../modules/organization/server/src/adapters/people-lifecycle-facts'
    );
    all.mockClear();
    const handle = db();
    const rows = [{id: 'n1', name: 'Finance', kind: 'department'}];
    all.mockResolvedValueOnce(rows);
    await expect(peopleLifecycleOrganizationFacts(handle).readPeopleLifecycleDataNodes('company-1')).resolves.toEqual({results: rows});
    expect(all).toHaveBeenCalledWith(handle, 'company-1', KIND, ['company-1']);
    expect(handle.prepare).not.toHaveBeenCalled();
  });

  it('pins the byte-identical SQL now owned by the Rust registry', () => {
    expect(sha(SQL)).toBe('540b876572f469cd4d0092a855ae17077479bf52f073e39a0f7fb49ef10ca2b8');
    const registry = read('organization_owner_write.rs');
    expect(registry).toContain(`pub const PEOPLE_LIFECYCLE_DATA_NODES: &str = "${KIND}";`);
    expect(registry).toContain(`    PEOPLE_LIFECYCLE_DATA_NODES,\n`);
    expect(registry).toContain(`        PEOPLE_LIFECYCLE_DATA_NODES => {\n            "${SQL}"\n        }`);
    const arm = registry.match(/PEOPLE_LIFECYCLE_DATA_NODES => \{\n            "([^"]+)"\n        \}/)?.[1] ?? '';
    expect(arm).toBe(SQL);
    expect(arm.toUpperCase().startsWith('SELECT ')).toBe(true);
    expect(arm).not.toMatch(/INSERT|UPDATE|DELETE|FOR UPDATE|FOR SHARE|LOCK TABLE/);
  });
});

describe('Organization lifecycle save position native cutover', () => {
  const srcRoot = resolve(__dirname, '../../../modules/organization/server/src');
  const read = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
  const sha = (value: string) => createHash('sha256').update(value).digest('hex');
  const KIND = 'lifecycle_read_save_position';
  const SQL = "SELECT level_id,branch_id,department_id FROM organization_positions WHERE company_id=? AND id=?";

  it('deletes the TS literal and sends the typed kind with both positional values', async () => {
    const source = read('people-lifecycle-facts.ts');
    expect(source).not.toContain(SQL);
    expect(source).toContain(`}>(database, companyId, '${KIND}', [companyId, positionId]);`);
    expect(source.split(KIND).length - 1).toBe(1);
    // The Company-owning row lock on the lifecycle position stays excluded.
    expect(source).toContain('FOR UPDATE');
    const {peopleLifecycleOrganizationFacts} = await import(
      '../../../modules/organization/server/src/adapters/people-lifecycle-facts'
    );
    one.mockClear();
    const handle = db();
    const row = {level_id: 'level', branch_id: null, department_id: 'department'};
    one.mockResolvedValueOnce(row);
    await expect(peopleLifecycleOrganizationFacts(handle).readSaveLifecyclePosition('company-1', 'position-1')).resolves.toBe(row);
    expect(one).toHaveBeenCalledWith(handle, 'company-1', KIND, ['company-1', 'position-1']);
    expect(handle.prepare).not.toHaveBeenCalled();
  });

  it('keeps the nullable single-row parity for a missing position', async () => {
    const {peopleLifecycleOrganizationFacts} = await import(
      '../../../modules/organization/server/src/adapters/people-lifecycle-facts'
    );
    one.mockClear();
    one.mockResolvedValueOnce(null);
    await expect(peopleLifecycleOrganizationFacts(db()).readSaveLifecyclePosition('company-1', 'position-1')).resolves.toBeNull();
  });

  it('pins the byte-identical SQL now owned by the Rust registry', () => {
    expect(sha(SQL)).toBe('3890b4ecb403e95cc95d4e0a80fb9ca8403a313d6b4b487e0a3247d4fa6c4911');
    const registry = read('organization_owner_write.rs');
    expect(registry).toContain(`pub const LIFECYCLE_READ_SAVE_POSITION: &str = "${KIND}";`);
    expect(registry).toContain(`    LIFECYCLE_READ_SAVE_POSITION,\n`);
    expect(registry).toContain(`        LIFECYCLE_READ_SAVE_POSITION => {\n            "${SQL}"\n        }`);
    const arm = registry.match(/LIFECYCLE_READ_SAVE_POSITION => \{\n            "([^"]+)"\n        \}/)?.[1] ?? '';
    expect(arm).toBe(SQL);
    expect(arm.toUpperCase().startsWith('SELECT ')).toBe(true);
    expect(arm).not.toMatch(/INSERT|UPDATE|DELETE|FOR UPDATE|FOR SHARE|LOCK TABLE/);
  });
});

describe('Organization lifecycle rehire position native cutover', () => {
  const srcRoot = resolve(__dirname, '../../../modules/organization/server/src');
  const read = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
  const sha = (value: string) => createHash('sha256').update(value).digest('hex');
  const KIND = 'lifecycle_read_rehire_position';
  const SQL = 'SELECT * FROM organization_positions WHERE company_id=? AND id=? AND active=1';

  it('deletes the TS literal/helper render and sends the typed kind with both values', async () => {
    const source = read('people-lifecycle-facts.ts');
    expect(source).not.toContain(SQL);
    expect(source).not.toContain('activePositionById');
    expect(source).toContain(`}>(database, companyId, '${KIND}', [companyId, positionId]);`);
    expect(source.split(KIND).length - 1).toBe(1);
    // The Company-owning FOR UPDATE lock on the lifecycle position stays excluded.
    expect(source).toContain('FOR UPDATE');
    const {peopleLifecycleOrganizationFacts} = await import(
      '../../../modules/organization/server/src/adapters/people-lifecycle-facts'
    );
    one.mockClear();
    const handle = db();
    const row = {branch_id: null, department_id: 'department', level_id: 'level', title: 'Engineer'};
    one.mockResolvedValueOnce(row);
    await expect(peopleLifecycleOrganizationFacts(handle).readCreateRehireEngagementPosition('company-1', 'position-1')).resolves.toBe(row);
    expect(one).toHaveBeenCalledWith(handle, 'company-1', KIND, ['company-1', 'position-1']);
    expect(handle.prepare).not.toHaveBeenCalled();
  });

  it('keeps the nullable single-row parity for an inactive or missing position', async () => {
    const {peopleLifecycleOrganizationFacts} = await import(
      '../../../modules/organization/server/src/adapters/people-lifecycle-facts'
    );
    one.mockClear();
    one.mockResolvedValueOnce(null);
    await expect(peopleLifecycleOrganizationFacts(db()).readCreateRehireEngagementPosition('company-1', 'position-1')).resolves.toBeNull();
  });

  it('pins the byte-identical SQL now owned by the Rust registry', () => {
    expect(sha(SQL)).toBe('5e19e3c48061ce902c502bdeb5668f2731ef1ce622754de690c0203bbbba87c3');
    const registry = read('organization_owner_write.rs');
    expect(registry).toContain(`pub const LIFECYCLE_READ_REHIRE_POSITION: &str = "${KIND}";`);
    expect(registry).toContain(`    LIFECYCLE_READ_REHIRE_POSITION,\n`);
    expect(registry).toContain(`        LIFECYCLE_READ_REHIRE_POSITION => {\n            "${SQL}"\n        }`);
    const rehire = registry.match(/LIFECYCLE_READ_REHIRE_POSITION => \{\n            "([^"]+)"\n        \}/)?.[1] ?? '';
    expect(rehire).toBe(SQL);
    expect(rehire.toUpperCase().startsWith('SELECT ')).toBe(true);
    expect(rehire).not.toMatch(/INSERT|UPDATE|DELETE|FOR UPDATE|FOR SHARE|LOCK TABLE/);
  });
});

describe('Organization lifecycle worker position native cutover', () => {
  const KIND = 'lifecycle_read_rehire_position';

  it('reuses the owner kind and preserves the pg QueryResult shape', async () => {
    const source = readFileSync(resolve(__dirname, '../../../modules/organization/server/src/adapters/people-lifecycle-worker-effects.ts'), 'utf8');
    expect(source).not.toContain('activePositionById');
    expect(source).not.toContain('organization-column-alias-families-r38');
    expect(source).toContain(`nativeOrgAll<LifecyclePositionFacts>(database, company_id, '${KIND}', [company_id, positionId]).then(rows => ({ rows, rowCount: rows.length }))`);
    const {lifecycleOrganizationFacts} = await import(
      '../../../modules/organization/server/src/adapters/people-lifecycle-worker-effects'
    );
    all.mockClear();
    const row = {branch_id: null, department_id: 'department', level_id: 'level', title: 'Engineer'};
    all.mockResolvedValueOnce([row]);
    await expect(lifecycleOrganizationFacts({} as never).readPosition('company-1', 'position-1')).resolves.toEqual({rows: [row], rowCount: 1});
    expect(all).toHaveBeenCalledWith(expect.anything(), 'company-1', KIND, ['company-1', 'position-1']);
    all.mockResolvedValueOnce([]);
    await expect(lifecycleOrganizationFacts({} as never).readPosition('company-1', null)).resolves.toEqual({rows: [], rowCount: 0});
  });
});

describe('Organization draft leave-defaults read native cutover', () => {
  const srcRoot = resolve(__dirname, '../../../modules/organization/server/src');
  const read = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
  const sha = (value: string) => createHash('sha256').update(value).digest('hex');
  const KIND = 'draft_read_leave_defaults';
  const SQL = 'SELECT level_id,version,policy_version_id FROM level_leave_defaults WHERE company_id=? ORDER BY level_id,version';

  it('deletes the TS literal and sends only the typed kind and Company value', () => {
    const source = read('organization-drafts.ts');
    expect(source).not.toContain(SQL);
    expect(source).toContain(`leave:await nativeOrgAll<{level_id:string;version:number;policy_version_id:string}>(db,companyId,'${KIND}',[companyId]),`);
    expect(source.split(KIND).length - 1).toBe(1);
    // The FOR UPDATE claim and the advisory-lock read stay excluded.
    expect(source).toContain('FOR UPDATE');
    expect(source).toContain('pg_try_advisory_xact_lock');
  });

  it('pins the byte-identical SQL now owned by the Rust registry', () => {
    expect(sha(SQL)).toBe('39766df8411791c865f7e18e2345f03211fd68b13bce02adc1d6b5b54409b7f7');
    const registry = read('organization_owner_write.rs');
    expect(registry).toContain(`pub const DRAFT_READ_LEAVE_DEFAULTS: &str = "${KIND}";`);
    expect(registry).toContain(`    DRAFT_READ_LEAVE_DEFAULTS,\n`);
    expect(registry).toContain(`        DRAFT_READ_LEAVE_DEFAULTS => {\n            "${SQL}"\n        }`);
    const arm = registry.match(/DRAFT_READ_LEAVE_DEFAULTS => \{\n            "([^"]+)"\n        \}/)?.[1] ?? '';
    expect(arm).toBe(SQL);
    expect(arm.toUpperCase().startsWith('SELECT ')).toBe(true);
    expect(arm).not.toMatch(/INSERT|UPDATE|DELETE|FOR UPDATE|FOR SHARE|LOCK TABLE/);
  });
});

describe('Organization change-event snapshot read native cutover', () => {
  const srcRoot = resolve(__dirname, '../../../modules/organization/server/src');
  const read = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
  const sha = (value: string) => createHash('sha256').update(value).digest('hex');
  const APPLIED = "SELECT snapshot_json FROM organization_node_change_events WHERE change_id=? AND action='applied' ORDER BY created_at DESC LIMIT 1";
  const SUBMITTED = "SELECT snapshot_json FROM organization_node_change_events WHERE change_id=? AND action='submitted' ORDER BY created_at DESC,id DESC LIMIT 1";

  it('deletes both TS renders and sends only typed kinds with the Company value', () => {
    const drafts = read('organization-drafts.ts');
    const org = read('organization.ts');
    expect(drafts).not.toContain(APPLIED);
    expect(org).not.toContain(SUBMITTED);
    expect(drafts).not.toContain('snapshotJsonByAction');
    expect(org).not.toContain('snapshotJsonByAction');
    expect(drafts).toContain("nativeOrgAll<{snapshot_json:string}>(db,companyId,'change_event_snapshot_applied',[id])");
    expect(org).toContain("nativeOrgAll<{snapshot_json:string}>(db,companyId,'change_event_snapshot_submitted',[id])");
    expect(drafts.split('change_event_snapshot_applied').length - 1).toBe(1);
    expect(org.split('change_event_snapshot_submitted').length - 1).toBe(1);
  });

  it('pins both byte-identical read-only statements in the Rust registry', () => {
    expect(sha(APPLIED)).toBe('c2440b239c1bd6e293774e4b15ebc6c9515966a4b787401fd5a3bc995f4cd880');
    expect(sha(SUBMITTED)).toBe('cf2d32fbb2e088beaddb7b0ada30f1f82ecfd2a3da5a261594ebcafbf15f3e7c');
    const registry = read('organization_owner_write.rs');
    for (const [kind, sql] of [
      ['CHANGE_EVENT_SNAPSHOT_APPLIED', APPLIED],
      ['CHANGE_EVENT_SNAPSHOT_SUBMITTED', SUBMITTED],
    ]) {
      expect(registry).toContain(`${kind}: &str = `);
      expect(registry).toContain(`        ${kind} => {\n            "${sql}"\n        }`);
    }
  });
});

describe('Organization attachment source image native cutover', () => {
  const srcRoot = resolve(__dirname, '../../../modules/organization/server/src');
  const read = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
  const sha = (value: string) => createHash('sha256').update(value).digest('hex');
  const KIND = 'attachment_organization_source_image';
  const SQL = "SELECT to_jsonb(company)::text image FROM organization_nodes company WHERE company.id=? AND company.kind='company'";

  it('deletes the TS literal and maps the typed single-row read to the image value', async () => {
    const source = read('attachment-read-sources.ts');
    expect(source).not.toContain(SQL);
    expect(source).toContain(`nativeOrgOne<{image:string}>(database,companyId,'${KIND}',[companyId]).then(row=>row?.image??null)`);
    expect(source.split(KIND).length - 1).toBe(1);
    // The table lock is a separate statement and stays byte-identical.
    expect(source).toContain('LOCK TABLE organization_nodes IN SHARE ROW EXCLUSIVE MODE');
    const {foundationAttachmentOrganizationSourceImage} = await import(
      '../../../modules/organization/server/src/adapters/attachment-read-sources'
    );
    one.mockClear();
    const handle = db();
    one.mockResolvedValueOnce({image: '{"id":"company"}'});
    await expect(foundationAttachmentOrganizationSourceImage(handle, 'company-1')).resolves.toBe('{"id":"company"}');
    expect(one).toHaveBeenCalledWith(handle, 'company-1', KIND, ['company-1']);
    expect(handle.prepare).not.toHaveBeenCalled();
    one.mockResolvedValueOnce(null);
    await expect(foundationAttachmentOrganizationSourceImage(handle, 'company-1')).resolves.toBeNull();
  });

  it('pins the byte-identical SQL now owned by the Rust registry', () => {
    expect(sha(SQL)).toBe('17d882597c91fc202c157067d2c6d6ef72fdc077bf010ba1fd4a356aee72aff1');
    const registry = read('organization_owner_write.rs');
    expect(registry).toContain(`pub const ATTACHMENT_ORGANIZATION_SOURCE_IMAGE: &str = "${KIND}";`);
    expect(registry).toContain(`    ATTACHMENT_ORGANIZATION_SOURCE_IMAGE,\n`);
    expect(registry).toContain(`        ATTACHMENT_ORGANIZATION_SOURCE_IMAGE => {\n            "${SQL}"\n        }`);
    const arm = registry.match(/ATTACHMENT_ORGANIZATION_SOURCE_IMAGE => \{\n            "([^"]+)"\n        \}/)?.[1] ?? '';
    expect(arm).toBe(SQL);
    expect(arm.toUpperCase().startsWith('SELECT ')).toBe(true);
    expect(arm).not.toMatch(/INSERT|UPDATE|DELETE|FOR UPDATE|FOR SHARE|LOCK TABLE/);
  });
});

describe('Organization attachment node/change source image native cutover', () => {
  const srcRoot = resolve(__dirname, '../../../modules/organization/server/src');
  const read = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
  const sha = (value: string) => createHash('sha256').update(value).digest('hex');
  const NODE_KIND = 'attachment_source_node_image';
  const CHANGE_KIND = 'attachment_source_change_image';
  const NODE = "SELECT jsonb_build_object('node',to_jsonb(node),'company',(SELECT to_jsonb(company) FROM organization_nodes company WHERE company.id=node.company_id AND company.kind='company'),\n 'branch',(SELECT to_jsonb(branch) FROM organization_nodes branch WHERE branch.id=node.branch_id AND branch.company_id=node.company_id))::text image FROM organization_nodes node WHERE node.id=? AND node.company_id=? AND node.kind IN('company','headquarters','branch','department')";
  const CHANGE = "SELECT jsonb_build_object('change',to_jsonb(change),'target',(SELECT to_jsonb(node) FROM organization_nodes node WHERE node.id=change.target_node_id AND node.company_id=change.company_id),\n 'branch',(SELECT to_jsonb(branch) FROM organization_nodes branch WHERE branch.id=change.branch_id AND branch.company_id=change.company_id),\n 'events',(SELECT jsonb_agg(to_jsonb(event) ORDER BY event.version,event.id) FROM organization_node_change_events event WHERE event.change_id=change.id AND event.company_id=change.company_id))::text image FROM organization_node_changes change WHERE change.id=? AND change.company_id=?";

  it('deletes both jsonb literals and sends typed kinds with id + Company', async () => {
    const source = read('attachment-read-sources.ts');
    expect(source).not.toContain("jsonb_build_object('node'");
    expect(source).not.toContain("jsonb_build_object('change'");
    expect(source).not.toContain('correlatedNodeJson');
    expect(source).toContain(`const kind=type==='organization_node'?'${NODE_KIND}':'${CHANGE_KIND}';`);
    const {organizationAttachmentReadSourceImage} = await import(
      '../../../modules/organization/server/src/adapters/attachment-read-sources'
    );
    one.mockClear();
    const handle = db();
    one.mockResolvedValueOnce({image: '{"node":1}'});
    await expect(organizationAttachmentReadSourceImage(handle, {companyId: 'company-1'} as never, 'organization_node', 'node-1')).resolves.toBe('{"node":1}');
    expect(one).toHaveBeenLastCalledWith(handle, 'company-1', NODE_KIND, ['node-1', 'company-1']);
    one.mockResolvedValueOnce({image: '{"change":1}'});
    await expect(organizationAttachmentReadSourceImage(handle, {companyId: 'company-1'} as never, 'organization_change', 'change-1')).resolves.toBe('{"change":1}');
    expect(one).toHaveBeenLastCalledWith(handle, 'company-1', CHANGE_KIND, ['change-1', 'company-1']);
    one.mockResolvedValueOnce(null);
    await expect(organizationAttachmentReadSourceImage(handle, {companyId: 'company-1'} as never, 'organization_node', 'missing')).resolves.toBeNull();
    one.mockClear();
    await expect(organizationAttachmentReadSourceImage(handle, {companyId: 'company-1'} as never, 'other', 'id')).resolves.toBeNull();
    expect(one).not.toHaveBeenCalled();
    expect(handle.prepare).not.toHaveBeenCalled();
  });

  it('pins both byte-identical statements in the Rust registry', () => {
    expect(sha(NODE)).toBe('4afd53d02e091f8a396967449f970ef417ca6fbef48631927a80ef4862c4869c');
    expect(sha(CHANGE)).toBe('8bf5a70d64fb60dec0e17c73402665ba99e52b0b9da09ee9d16f4e14070b3769');
    const registry = read('organization_owner_write.rs');
    for (const [kind, sql] of [
      ['ATTACHMENT_SOURCE_NODE_IMAGE', NODE],
      ['ATTACHMENT_SOURCE_CHANGE_IMAGE', CHANGE],
    ]) {
      const escaped = sql.replace(/\n/g, '\\n');
      expect(registry).toContain(`${kind}: &str = `);
      expect(registry).toContain(`        ${kind} => {\n            "${escaped}"\n        }`);
    }
  });
});

describe('Organization staff settings levels read native cutover', () => {
  const srcRoot = resolve(__dirname, '../../../modules/organization/server/src');
  const read = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
  const sha = (value: string) => createHash('sha256').update(value).digest('hex');
  const KIND = 'staff_settings_list_levels';
  const SQL = 'SELECT * FROM staff_levels WHERE company_id=? ORDER BY level_number ASC NULLS LAST,rank DESC,code';

  it('deletes the TS literal and keeps the retained {results} shape', async () => {
    const source = read('operations-staff-settings-facts.ts');
    expect(source).not.toContain(SQL);
    expect(source).toContain(`}>(database, companyId, '${KIND}', [companyId]).then(results => ({ results }));`);
    expect(source.split(KIND).length - 1).toBe(1);
    // The version guard and department lookup stay untouched.
    expect(source).toContain('ownerVersionGuardSource');
    expect(source).toContain('nodeKindByIdCompany');
    const {StaffSettingsOrganizationFacts} = await import(
      '../../../modules/organization/server/src/adapters/operations-staff-settings-facts'
    );
    all.mockClear();
    const handle = db();
    const rows = [{id: 'level', company_id: 'company-1', code: 'L1'}];
    all.mockResolvedValueOnce(rows);
    await expect(StaffSettingsOrganizationFacts(handle).readListLevelsEffect('company-1')).resolves.toEqual({results: rows});
    expect(all).toHaveBeenCalledWith(handle, 'company-1', KIND, ['company-1']);
    expect(handle.prepare).not.toHaveBeenCalled();
  });

  it('pins the byte-identical SQL now owned by the Rust registry', () => {
    expect(sha(SQL)).toBe('b1fdab87dcf08acf4996e6279eae4cf67715507f6d61d3d448fbdc4a1453bfdf');
    const registry = read('organization_owner_write.rs');
    expect(registry).toContain(`pub const STAFF_SETTINGS_LIST_LEVELS: &str = "${KIND}";`);
    expect(registry).toContain(`    STAFF_SETTINGS_LIST_LEVELS,\n`);
    expect(registry).toContain(`        STAFF_SETTINGS_LIST_LEVELS => {\n            "${SQL}"\n        }`);
    const arm = registry.match(/STAFF_SETTINGS_LIST_LEVELS => \{\n            "([^"]+)"\n        \}/)?.[1] ?? '';
    expect(arm).toBe(SQL);
    expect(arm.toUpperCase().startsWith('SELECT ')).toBe(true);
    expect(arm).not.toMatch(/INSERT|UPDATE|DELETE|FOR UPDATE|FOR SHARE|LOCK TABLE/);
  });
});

describe('Organization staff policy request route head native cutover', () => {
  const srcRoot = resolve(__dirname, '../../../modules/organization/server/src');
  const read = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
  const sha = (value: string) => createHash('sha256').update(value).digest('hex');
  const KIND = 'staff_policy_request_route_head';
  const SQL = "SELECT h.person_id,h.version FROM department_heads h JOIN organization_nodes n ON n.id=h.department_id WHERE h.company_id=? AND h.department_id=? AND n.company_id=h.company_id AND n.status='active' AND n.kind='department'";

  it('deletes the TS literal and returns the typed head row or null', async () => {
    const source = read('operations-staff-policy-facts.ts');
    expect(source).not.toContain(SQL);
    expect(source).toContain(`}>(database, companyId, '${KIND}', [companyId, department_id]);`);
    expect(source.split(KIND).length - 1).toBe(1);
    const {StaffPolicyOrganizationFacts} = await import(
      '../../../modules/organization/server/src/adapters/operations-staff-policy-facts'
    );
    one.mockClear();
    const handle = db();
    const row = {person_id: 'person', version: 2};
    one.mockResolvedValueOnce(row);
    await expect(StaffPolicyOrganizationFacts(handle).readRequestRouteHead('company-1', 'department-1')).resolves.toBe(row);
    expect(one).toHaveBeenCalledWith(handle, 'company-1', KIND, ['company-1', 'department-1']);
    one.mockResolvedValueOnce(null);
    await expect(StaffPolicyOrganizationFacts(handle).readRequestRouteHead('company-1', 'department-1')).resolves.toBeNull();
    expect(handle.prepare).not.toHaveBeenCalled();
  });

  it('pins the byte-identical SQL now owned by the Rust registry', () => {
    expect(sha(SQL)).toBe('03c113f88d99c3cfaa8ee4f03f8616f8a0a32df2f3efe037b31e5d57f104d250');
    const registry = read('organization_owner_write.rs');
    expect(registry).toContain(`pub const STAFF_POLICY_REQUEST_ROUTE_HEAD: &str = "${KIND}";`);
    expect(registry).toContain(`    STAFF_POLICY_REQUEST_ROUTE_HEAD,\n`);
    expect(registry).toContain(`        STAFF_POLICY_REQUEST_ROUTE_HEAD => {\n            "${SQL}"\n        }`);
    const arm = registry.match(/STAFF_POLICY_REQUEST_ROUTE_HEAD => \{\n            "([^"]+)"\n        \}/)?.[1] ?? '';
    expect(arm).toBe(SQL);
    expect(arm.toUpperCase().startsWith('SELECT ')).toBe(true);
    expect(arm).not.toMatch(/INSERT|UPDATE|DELETE|FOR UPDATE|FOR SHARE|LOCK TABLE/);
  });
});

describe('Organization draft request list native cutover', () => {
  const srcRoot = resolve(__dirname, '../../../modules/organization/server/src');
  const read = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
  const source = (path: string) => readFileSync(resolve(srcRoot, path), 'utf8');
  const sha = (value: string) => createHash('sha256').update(value).digest('hex');
  const KIND = 'draft_read_requests';
  const SQL = 'SELECT id,item_keys,state,last_error,change_id FROM organization_draft_requests WHERE draft_id=? ORDER BY created_at,id';

  it('deletes the TS literal and threads the Company through the requests port', () => {
    const drafts = read('organization-drafts.ts');
    expect(drafts).not.toContain(SQL);
    expect(drafts).toContain(`requests:async(companyId,id)=>nativeOrgAll<DraftRequest>(db,companyId,'${KIND}',[id]),`);
    expect(drafts.split(KIND).length - 1).toBe(1);
    const app = source('application/organization-drafts.ts');
    expect(app).toContain('requests(companyId:string,id:string):Promise<DraftRequest[]>');
    expect(app).toContain('ports.requests(actor.companyId,draft.id)');
    // The FOR UPDATE claim and advisory-lock read stay excluded.
    expect(drafts).toContain('FOR UPDATE');
    expect(drafts).toContain('pg_try_advisory_xact_lock');
  });

  it('pins the byte-identical SQL now owned by the Rust registry', () => {
    expect(sha(SQL)).toBe('acd62f94cd510e0d5fc8d05fa9bb9b0b1e53e88c28810a9669f85d479fcb7438');
    const registry = read('organization_owner_write.rs');
    expect(registry).toContain(`pub const DRAFT_READ_REQUESTS: &str = "${KIND}";`);
    expect(registry).toContain(`    DRAFT_READ_REQUESTS,\n`);
    expect(registry).toContain(`        DRAFT_READ_REQUESTS => {\n            "${SQL}"\n        }`);
    const arm = registry.match(/DRAFT_READ_REQUESTS => \{\n            "([^"]+)"\n        \}/)?.[1] ?? '';
    expect(arm).toBe(SQL);
    expect(arm.toUpperCase().startsWith('SELECT ')).toBe(true);
    expect(arm).not.toMatch(/INSERT|UPDATE|DELETE|FOR UPDATE|FOR SHARE|LOCK TABLE/);
  });
});
