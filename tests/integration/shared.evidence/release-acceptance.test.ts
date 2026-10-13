import { beforeAll, expect, it, vi } from 'vitest';
import { applyMigrations, env } from "../../support/runtime-env";
import { expenseFixture, expenseDb as db, key } from "../../support/helpers/expense-fixture";
import { productionScope, releaseCriteria } from "../../../src/data/production-scope";
import { acceptedReleasePredicate, companyReleaseStatus, currentReleaseInputs, releaseAcceptance, submitReleaseAcceptance, reviewReleaseAcceptance, revokeReleaseAcceptance } from "../../../src/server/modules/platform_administration/public/release-acceptance";
import { changeCompanyFeatureRollout, assertProductionModuleEnabled, enabledProductionModules, productionFeatureGuard, scheduledFeaturePredicate } from "../../../src/server/modules/pms/public/hospitality-rollout";
import type { WorkContext } from "../../../src/server/modules/identity_access/public/authorization";
import { mkdtemp, mkdir, readdir, symlink, rm } from 'node:fs/promises';
import { resolve, join } from 'node:path';
import { migrateDatabase } from "../../../src/server/modules/runtime/adapters/runtime/migrations";

vi.mock("../../../src/data/release-artifact.json", () => ({ default: { softwareVersion: 'release-integration-fixture' } }));
vi.mock("../../../src/server/modules/runtime/adapters/runtime", () => ({ holdingResources: async () => ({ write: env.H001_DB }), runtimeEnv: async () => env }));
beforeAll(async () => { await applyMigrations(db, env.STAFF_TEST_MIGRATIONS); });

async function fixture() {
  const f = await expenseFixture();
  const reviewer: WorkContext = { ...f.head, role: 'holding_owner', branchId: null, departmentId: null,
    assignmentId: await f.grant(f.head, 'holding_owner'), scope: f.owner.scope, modules: f.owner.modules };
  // A pre-existing setup attestation is the upgrade boundary, never a release.
  await db.prepare(`INSERT INTO company_business_acceptance(company_id,status,version,requested_by,reviewed_by,requested_at,reviewed_at,reason,review_reason,updated_at)
    VALUES(?,'accepted',2,?,?,1,2,'Fixture setup','Independent fixture setup',2)`).bind(f.company, f.owner.userId, reviewer.userId).run();
  const inputs = await currentReleaseInputs(db, f.company);
  const submission = { scopeVersion: productionScope.version, softwareVersion: inputs.softwareVersion,
    expectedConfigurationRevision: inputs.configurationRevision, expectedSetupVersion: inputs.setupVersion,
    reason: 'Isolated test evidence; no real hotel or statutory sign-off', evidence: releaseCriteria.map(row => ({
      criterionId: row.id, summary: 'Synthetic acceptance fixture only', files: [{ reference: `fixture:${row.id}`, sha256: 'a'.repeat(64) }],
    })) };
  return { ...f, reviewer, submission };
}

it('keeps existing setup and generated observations separate, with immutable release evidence and idempotency', async () => {
  const f = await fixture();
  expect(await companyReleaseStatus(db, f.company)).toMatchObject({ state: 'pending', release: null, inputs: { setupAccepted: true } });
  const command = key();
  const release = await submitReleaseAcceptance(db, f.owner, f.submission, command);
  expect(await submitReleaseAcceptance(db, f.owner, f.submission, command)).toEqual(release);
  await expect(submitReleaseAcceptance(db, f.owner, { ...f.submission, reason: 'Changed content with the same key' }, command)).rejects.toMatchObject({ code: 'IDEMPOTENCY_CONFLICT' });
  expect(await companyReleaseStatus(db, f.company)).toMatchObject({ state: 'pending', release: { id: release.id, version: 1 } });
  await expect(db.prepare("UPDATE company_release_acceptances SET evidence_json='[]' WHERE id=?").bind(release.id).run()).rejects.toThrow('RELEASE_SNAPSHOT_IMMUTABLE');
  await expect(db.prepare('DELETE FROM company_release_events WHERE release_id=?').bind(release.id).run()).rejects.toThrow('RELEASE_EVIDENCE_IMMUTABLE');
  // Reapplying the additive migration preserves old setup records and new attestations.
  await applyMigrations(db, env.STAFF_TEST_MIGRATIONS);
  expect(await currentReleaseInputs(db, f.company)).toMatchObject({ setupVersion: 2, setupAccepted: true });
  expect((await companyReleaseStatus(db, f.company)).release?.id).toBe(release.id);
});

it('rejects self review, stale versions, unavailable authority, out-of-scope identities and missing/duplicate criteria', async () => {
  const f = await fixture();
  await expect(submitReleaseAcceptance(db, f.owner, { ...f.submission, evidence: Array(releaseCriteria.length).fill(f.submission.evidence[0]) }, key())).rejects.toThrow();
  const release = await submitReleaseAcceptance(db, f.owner, f.submission, key());
  const review = { expectedVersion: 1, criterionId: 'documentation', state: 'passed', reason: 'Checked evidence independently' };
  await expect(reviewReleaseAcceptance(db, f.owner, String(release.id), review, key())).rejects.toMatchObject({ code: 'INDEPENDENT_REVIEW_REQUIRED' });
  await expect(reviewReleaseAcceptance(db, f.claimant, String(release.id), review, key())).rejects.toMatchObject({ status: 403 });
  await expect(releaseAcceptance(db, { ...f.reviewer, branchId: f.branch }, String(release.id))).rejects.toMatchObject({ status: 403 });
  const other = await fixture();
  await expect(releaseAcceptance(db, other.owner, String(release.id))).rejects.toMatchObject({ status: 404 });
  const reviewKey = key();
  const result = await reviewReleaseAcceptance(db, f.reviewer, String(release.id), review, reviewKey);
  expect(await reviewReleaseAcceptance(db, f.reviewer, String(release.id), review, reviewKey)).toEqual(result);
  await expect(reviewReleaseAcceptance(db, f.reviewer, String(release.id), review, key())).rejects.toMatchObject({ code: 'STALE_VERSION' });
  await expect(reviewReleaseAcceptance(db, f.reviewer, String(release.id), { ...review, expectedVersion: 2, state: 'not_applicable' }, key())).rejects.toThrow();
  await db.prepare('UPDATE assignments SET revoked_at=? WHERE id=?').bind(Date.now(), f.reviewer.assignmentId).run();
  await expect(reviewReleaseAcceptance(db, f.reviewer, String(release.id), review, reviewKey)).rejects.toMatchObject({ status: 403 });
  expect((await companyReleaseStatus(db, f.company)).gates.find(row => row.id === 'documentation')?.state).toBe('pending');
});

it('requires every scope criterion and invalidates UI/API/owner/worker eligibility on material changes and revocation', async () => {
  const f = await fixture();
  const release = await submitReleaseAcceptance(db, f.owner, f.submission, key());
  let version = 1;
  for (const criterion of releaseCriteria) {
    expect((await companyReleaseStatus(db, f.company)).state).toBe('pending');
    await reviewReleaseAcceptance(db, f.reviewer, String(release.id), { expectedVersion: version++, criterionId: criterion.id, state: 'passed', reason: 'Independent isolated fixture review' }, key());
  }
  expect((await companyReleaseStatus(db, f.company)).state).toBe('passed');
  expect(await db.prepare(`SELECT ${acceptedReleasePredicate('?')} AS accepted`).bind(f.company).first()).toEqual({ accepted: true });
  vi.stubEnv('NODE_ENV', 'production');
  try {
    await changeCompanyFeatureRollout(f.owner, { moduleId: 'pos', expectedVersion: 0, state: 'available', reason: 'Fixture review completed' }, key());
    await expect(assertProductionModuleEnabled(db, f.company, 'pos')).resolves.toBeUndefined();
    expect((await enabledProductionModules(db, f.company)).has('pos')).toBe(true);
    expect(await db.prepare(`SELECT ${scheduledFeaturePredicate('?', 'pos')} AS enabled`).bind(f.company).first()).toEqual({ enabled: true });
    await db.prepare("UPDATE finance_charts SET name=name||' revised' WHERE company_id=?").bind(f.company).run();
    expect((await companyReleaseStatus(db, f.company)).blockers).toContain('Material Company configuration changed.');
    await expect(assertProductionModuleEnabled(db, f.company, 'pos')).rejects.toMatchObject({ code: 'FEATURE_COMING_SOON' });
    expect((await enabledProductionModules(db, f.company)).has('pos')).toBe(false);
    expect(await db.prepare(`SELECT ${scheduledFeaturePredicate('?', 'pos')} AS enabled`).bind(f.company).first()).toEqual({ enabled: false });
    await expect(db.batch(productionFeatureGuard(db, key(), f.company, 'pos'))).rejects.toThrow();
    await expect(reviewReleaseAcceptance(db, f.reviewer, String(release.id), { expectedVersion: version, criterionId: 'documentation', state: 'passed', reason: 'Cannot reuse old configuration' }, key())).rejects.toMatchObject({ code: 'RELEASE_INPUTS_CHANGED' });
    const revokeKey = key(), input = { expectedVersion: version, reason: 'Withdraw this stale fixture release' };
    const revoked = await revokeReleaseAcceptance(db, f.owner, String(release.id), input, revokeKey);
    expect(await revokeReleaseAcceptance(db, f.owner, String(release.id), input, revokeKey)).toEqual(revoked);
    expect((await companyReleaseStatus(db, f.company)).state).toBe('pending');
  } finally { vi.unstubAllEnvs(); }
});

it('serializes concurrent reviewers so one expected version produces only one decision event', async () => {
  const f = await fixture();
  const release = await submitReleaseAcceptance(db, f.owner, f.submission, key());
  const results = await Promise.allSettled(['documentation', 'release'].map(criterionId => reviewReleaseAcceptance(db, f.reviewer, String(release.id), {
    expectedVersion: 1, criterionId, state: 'passed', reason: 'Concurrent independent review',
  }, key())));
  expect(results.filter(result => result.status === 'fulfilled')).toHaveLength(1);
  expect((await companyReleaseStatus(db, f.company)).release?.version).toBe(2);
  expect(await db.prepare("SELECT count(*) count FROM company_release_events WHERE release_id=? AND action='reviewed'").bind(release.id).first()).toEqual({ count: 1 });
});

it('marks historical releases as superseded and rejects reviews against them', async () => {
  const f = await fixture();
  const oldRelease = await submitReleaseAcceptance(db, f.owner, f.submission, key());
  await new Promise(resolve => setTimeout(resolve, 2));
  const currentRelease = await submitReleaseAcceptance(db, f.owner, f.submission, key());
  expect((await companyReleaseStatus(db, f.company)).release?.id).toBe(currentRelease.id);
  expect((await companyReleaseStatus(db, f.company, String(oldRelease.id))).blockers)
    .toContain('A newer release superseded this acceptance.');
  await expect(reviewReleaseAcceptance(db, f.reviewer, String(oldRelease.id), {
    expectedVersion: 1, criterionId: 'documentation', state: 'passed', reason: 'Old evidence cannot be reviewed',
  }, key())).rejects.toMatchObject({ code: 'RELEASE_SUPERSEDED' });
});

it('blocks non-THB settings even when the submitted configuration version is current', async () => {
  const f = await fixture();
  await db.prepare("UPDATE finance_charts SET base_currency='USD' WHERE company_id=?").bind(f.company).run();
  const current = await currentReleaseInputs(db, f.company);
  expect(current.currencyCompatible).toBe(false);
  await expect(submitReleaseAcceptance(db, f.owner, { ...f.submission, expectedConfigurationRevision: current.configurationRevision }, key())).rejects.toMatchObject({ code: 'RELEASE_INPUTS_CHANGED' });
});

it('upgrades an existing database without promoting or replacing its setup attestation', async () => {
  const temporary = await mkdtemp(resolve('.runtime/tests/release-upgrade-'));
  const upgraded = env.H002_DB;
  try {
    await mkdir(join(temporary, 'holding-write'));
    for (const file of await readdir('migrations/postgres/holding-write')) {
      // Build the actual pre-release migration boundary; later migrations may
      // depend on the release tables and must be installed during upgrade.
      if (!file.endsWith('.sql') || Number(file.split('_')[0]) >= 304) continue;
      await symlink(resolve('migrations/postgres/holding-write', file), join(temporary, 'holding-write', file));
    }
    await migrateDatabase(upgraded, 'holding-write', temporary);
    await upgraded.prepare("INSERT INTO organization_nodes(id,kind,slug,display_name,company_id) VALUES('upgrade-company','company','upgrade-company','Upgrade fixture','upgrade-company')").run();
    await upgraded.prepare("INSERT INTO company_business_acceptance(company_id,status,version,requested_by,requested_at,reason,updated_at) VALUES('upgrade-company','accepted',7,'historical-requester',1,'Previously accepted setup',1)").run();
    await migrateDatabase(upgraded, 'holding-write');
    expect(await upgraded.prepare("SELECT status,version,requested_by FROM company_business_acceptance WHERE company_id='upgrade-company'").first()).toEqual({ status: 'accepted', version: 7, requested_by: 'historical-requester' });
    expect(await upgraded.prepare('SELECT count(*) count FROM company_release_acceptances').first()).toEqual({ count: 0 });
    expect(await upgraded.prepare("SELECT revision FROM company_release_revisions WHERE company_id='upgrade-company'").first()).toEqual({ revision: 0 });
  } finally { await rm(temporary, { recursive: true, force: true }); }
});
