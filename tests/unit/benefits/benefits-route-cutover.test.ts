import { existsSync, readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { describe, expect, it, vi } from 'vitest';
import { holdingHosts } from '../../../packages/contracts/src/holding-hosts';
import { expandNativeApiPaths, loadNativeApiPaths } from '../../../scripts/local/native-api-paths';
import { nativeApiPath } from '../../../scripts/local/native-api-proxy';
vi.mock('server-only', () => ({}));
import {
  createNativeBenefitsClient,
  nativeBenefitsConfiguration,
  type NativeBenefitsPageContext,
} from '../../../packages/feature-benefits/src/public/native-benefits-workspace';

const read = (file: string) => readFileSync(fileURLToPath(new URL(`../../../${file}`, import.meta.url)), 'utf8');
const exists = (file: string) => existsSync(fileURLToPath(new URL(`../../../${file}`, import.meta.url)));
const checkedIn = loadNativeApiPaths(fileURLToPath(new URL('../../../scripts/local/native-api-paths.json', import.meta.url)));
const environment = { NODE_ENV: 'test' as const, ERPX_DEV_NATIVE_PATHS: expandNativeApiPaths(checkedIn, holdingHosts).paths };

const config = JSON.stringify({
  privateBaseUrl: 'http://127.0.0.1:44997',
  timeoutMs: 1000,
  holdings: [{ origin: 'https://acme.example.com', holdingSlug: 'acme', holdingId: 'h001' }],
});
const incoming = (overrides: Record<string, string> = {}) => new Headers({ host: 'acme.example.com', cookie: '__Host-erpx-product=session', ...overrides });
const actor: NativeBenefitsPageContext = {
  userId: 'user', holdingId: 'h001', holdingSlug: 'acme', companyId: 'company',
  branchId: null, departmentId: null, assignmentId: 'assignment', role: 'owner',
  assignmentScope: 'holding', scope: { kind: 'holding' }, authority: { revision: 7, assignmentVersion: 3 },
  modules: ['benefits'], iam: { permissions: ['records.read', 'records.write'] },
};
const context = {
  userId: 'user', holdingId: 'h001', companyId: 'company', branchId: null,
  departmentId: null, branchSlug: null, departmentSlug: null, assignmentId: 'assignment',
  assignmentVersion: 3, revision: 7, role: 'owner', assignmentScopeKind: 'holding',
  modules: ['benefits'], permissions: ['records.read', 'records.write'],
  expiresAt: 9_007_199_254_740_991,
};
const plan = {
  id: 'plan-family', company_id: 'company', name: 'Family medical plan', provider: 'Synthetic provider',
  coverage_type: 'family', currency: 'THB', employee_deduction_minor: 12550, employer_cost_minor: 30000,
  effective_from: '2026-01-01', effective_to: '2026-12-31', status: 'published', version: 1,
  created_by: 'owner', reviewed_by: 'reviewer', created_at: 10, updated_at: 20,
  required_eligibility_code: 'PROBATION_COMPLETE',
};
const windowRow = {
  id: 'window-annual', company_id: 'company', name: 'Annual enrollment', opens_on: '2026-11-01',
  closes_on: '2026-11-30', effective_on: '2027-01-01', status: 'open', version: 1,
  created_by: 'owner', reviewed_by: 'reviewer', created_at: 10, updated_at: 30,
};
const dependentRow = {
  id: 'dependent-one', company_id: 'company', person_id: 'person-owner', name: 'Synthetic dependent',
  relationship: 'child', birth_date: '2018-01-01', status: 'verified', version: 1,
  created_by: 'owner', reviewed_by: 'reviewer', created_at: 10, updated_at: 40,
};
const electionRow = {
  id: 'election-owner', company_id: 'company', person_id: 'person-owner', engagement_id: 'engagement-owner',
  plan_id: 'plan-family', plan_name: 'Family medical plan', employee_name: 'Synthetic Owner',
  plan_version: 1, window_id: 'window-annual', window_version: 1, coverage_type: 'family', currency: 'THB',
  employee_deduction_minor: 12550, employer_cost_minor: 30000, effective_from: '2027-01-01',
  effective_to: '2027-12-31', status: 'elected', version: 1, created_at: 10, updated_at: 50,
};
const workspace = {
  actorId: 'user', canWrite: true, canReview: false, canManageDocuments: false,
  workContext: 'a'.repeat(64), plans: [plan], windows: [windowRow],
  dependents: [dependentRow], elections: [electionRow], purchase: null,
};
const detail = {
  election: { id: 'election-owner', company_id: 'company', person_id: 'person-owner', status: 'released' },
  history: [], payroll: null, canReview: false, canManageDocuments: false, isOwner: true,
  actorId: 'user', canWrite: true, workContext: 'a'.repeat(64),
  purchase: { newWork: true, settleExisting: true, history: true, revision: '7' },
};
const input = {
  holdingSlug: 'acme', personal: false,
  expected: { companyId: 'company', assignmentId: 'assignment', revision: 7, assignmentVersion: 3 },
};
const ok = (value: unknown) => new Response(JSON.stringify(value), { status: 200, headers: { 'content-type': 'application/json' } });

describe('Benefits Next route retirement', () => {
  it('keeps the shared GET/POST handler and both app re-exports unresolvable', () => {
    for (const file of [
      'packages/feature-people/src/routes/api/v1/holdings/[holdingSlug]/benefits/route.ts',
      'apps/people/src/app/api/v1/holdings/[holdingSlug]/benefits/route.ts',
      'apps/compat/src/app/api/v1/holdings/[holdingSlug]/benefits/route.ts',
    ]) expect(exists(file), file).toBe(false);
    expect(exists('packages/server-benefits/src/adapters/workspace_http.rs')).toBe(true);
    expect(exists('packages/server-benefits/src/adapters/command_http.rs')).toBe(true);
  });

  it('matches the checked-in benefits prefix for every served method and exact detail path', () => {
    expect(checkedIn.holdingPrefixes).toContain('benefits');
    for (const { hostname, holdingSlug } of holdingHosts) {
      const base = `/api/v1/holdings/${holdingSlug}/benefits`;
      expect(nativeApiPath(environment, `${base}?id=election-owner`, hostname, 'GET'), hostname).toBe(true);
      expect(nativeApiPath(environment, `${base}/election-owner`, hostname, 'GET'), hostname).toBe(true);
      expect(nativeApiPath(environment, base, hostname, 'POST'), hostname).toBe(true);
      // Retained command-plan surface still matches the same native prefix.
      expect(nativeApiPath(environment, `${base}/command-plan`, hostname, 'POST'), hostname).toBe(true);
      expect(nativeApiPath(environment, base, 'unregistered.example.test', 'GET')).toBe(false);
    }
  });

  it('executes Benefits SQL in the native owner and keeps the exact {data, context} payload', () => {
    const workspaceSql = read('packages/server-benefits/src/adapters/workspace.rs');
    expect(workspaceSql).toContain('sqlx::query(');
    expect(workspaceSql).toContain('.fetch_all(&mut *self.connection)');
    const commandSql = read('packages/server-benefits/src/adapters/command.rs');
    expect(commandSql).toContain('sqlx::query(');
    expect(commandSql).toContain('.execute(&mut *self.connection)');
    const reads = read('packages/server-benefits/src/adapters/workspace_http.rs');
    expect(reads).toContain('"/api/v1/holdings/{holding_slug}/benefits"');
    expect(reads).toContain('"/api/v1/holdings/{holding_slug}/benefits/{id}"');
    expect(reads).toContain('get(read_detail)');
    expect(reads).toContain('"data": self.data, "context": self.context');
    const commands = read('packages/server-benefits/src/adapters/command_http.rs');
    expect(commands).toContain('"/api/v1/holdings/{holding_slug}/benefits"');
    expect(commands).toContain('post(run_command)');
    expect(commands).toContain('"data": self.data, "context": self.context');
    const client = read('packages/feature-benefits/src/public/native-benefits-workspace.ts');
    expect(client).toContain('/benefits/${encodeURIComponent(detail)}');
    expect(client).toContain("exact(value, ['data', 'context'])");
    expect(client).not.toContain('?id=');
    const browser = read('packages/feature-benefits/src/lib/command.ts');
    expect(browser).toContain('/api/v1${base}/benefits');
  });

  it('returns native workspace/detail payloads and fails closed on foreign authority or context', async () => {
    const transport = vi.fn().mockResolvedValue(ok({ data: workspace, context }));
    const client = createNativeBenefitsClient(nativeBenefitsConfiguration(config), transport);
    expect(await client.workspace(incoming(), actor, input)).toMatchObject({
      workContext: 'a'.repeat(64), plans: [plan], elections: [electionRow],
    });
    expect(transport.mock.calls[0][1].toString()).toBe('http://127.0.0.1:44997/api/v1/holdings/acme/benefits');

    const detailTransport = vi.fn().mockResolvedValue(ok({ data: detail, context }));
    const detailClient = createNativeBenefitsClient(nativeBenefitsConfiguration(config), detailTransport);
    expect(await detailClient.detail(incoming(), actor, input, 'election-owner')).toEqual(detail);
    expect(detailTransport.mock.calls[0][1].toString()).toBe('http://127.0.0.1:44997/api/v1/holdings/acme/benefits/election-owner');
    expect(detailTransport.mock.calls[0][1].search).toBe('');

    const foreign = createNativeBenefitsClient(nativeBenefitsConfiguration(config), detailTransport);
    await expect(foreign.detail(incoming({ host: 'evil.example.com' }), actor, input, 'election-owner'))
      .rejects.toMatchObject({ code: 'NATIVE_BENEFITS_AUTHORITY_INVALID' });
    const changed = createNativeBenefitsClient(
      nativeBenefitsConfiguration(config),
      vi.fn().mockResolvedValue(ok({ data: detail, context: { ...context, companyId: 'foreign' } })),
    );
    await expect(changed.detail(incoming(), actor, input, 'election-owner'))
      .rejects.toMatchObject({ code: 'NATIVE_BENEFITS_CONTEXT_CHANGED' });
  });
});
