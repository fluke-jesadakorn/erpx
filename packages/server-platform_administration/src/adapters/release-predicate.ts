import { platformOwnerProjectionSources as financePlatformSources } from "@erpx/server-finance/public/platform-owner-projection-sources";
import { platformOwnerProjectionSources as commercePlatformSources } from "@erpx/server-commerce/public/platform-owner-projection-sources";
import { platformOwnerProjectionSources as posPlatformSources } from "@erpx/server-pos/public/platform-owner-projection-sources";
import { platformOwnerProjectionSources as pmsPlatformSources } from "@erpx/server-pms/public/platform-owner-projection-sources";
import { platformOwnerProjectionSources as people_payrollPlatformSources } from "@erpx/server-people_payroll/public/platform-owner-projection-sources";
import { platformOwnerProjectionSources as identity_accessPlatformSources } from "@erpx/server-identity_access/public/platform-owner-projection-sources";
import { platformOwnerProjectionSources as organizationPlatformSources } from "@erpx/server-organization/public/platform-owner-projection-sources";
import "server-only";
import artifact from "@erpx/catalogue/release-artifact.json";
import {productionScope,legalProductionScope,criteriaForScope,type ReleaseScopeVersion} from "@erpx/catalogue/production-scope";
import {fillReadHead,platformReadHeads} from "./platform-read-heads";

export function releaseRevisionTable(scope: ReleaseScopeVersion) {
  return scope === legalProductionScope.version ? 'company_legal_release_revisions' : 'company_release_revisions';
}

/** The single release-acceptance existence guard shared by the version, latest
 * and attachment-owner checks. Kept here once so this parameterized family has
 * one owner-local definition; each caller supplies only its WHERE predicate.
 * The rendered SQL is byte-identical to the former inline copies. */
export function releaseAcceptanceExistsSql(where: string) {
  return releaseTableExistsSql('company_release_acceptances', where);
}

/** The single owner-local one-row `FROM … WHERE …` head shared by every table
 * EXISTS guard and derived-source probe. Both callers previously inlined the
 * same head; this one definition renders byte-identical SQL for each and only
 * FROM, WHERE and their separator vary. Fixed-table wrappers, projection reads,
 * row heads, locks and native reads are unchanged. */
export function selectOneFromWhereSql(from: string, where: string, gap = ' ') {
  return fillReadHead(platformReadHeads.selectOneFromWhere, { from, gap, where });
}

/** The parameterized EXISTS SELECT-head shared by the release-configuration
 * guard and the PMS rollout feature-release guard. It composes the shared
 * SELECT-1 head; the rendered SQL stays byte-identical for every caller. */
export function releaseTableExistsSql(from: string, where: string, gap = ' ') {
  return `EXISTS(${selectOneFromWhereSql(from, where, gap)})`;
}

/** The derived-source EXISTS row head shared by the reviews authority guard and
 * the required-permission gate. It composes the same SELECT-1 head with a
 * derived FROM expression; every rendering stays byte-identical. */
export function derivedSourceExistsSql(source: string, alias: string, predicate: string) {
  return selectOneFromWhereSql(`(${source}) ${alias}`, predicate);
}

/** The revision read is the same parameterized row-head as the acceptance read;
 * the scope table is the only difference. It now composes the shared renderer,
 * so callers still supply just a suffix (e.g. ' FOR SHARE') and every rendering
 * stays byte-identical. */
export function releaseRevisionReadSql(scope: ReleaseScopeVersion, suffix = '') {
  return releaseAcceptanceRowSql('revision', 'company_id=?', suffix, releaseRevisionTable(scope));
}

/** The single revision share-lock statement shared by release acceptance and
 * PMS rollout guards. Kept here once so owner guards never repeat the same
 * fixed lock SQL in every call site. */
export function releaseRevisionShareLockSql(scope: ReleaseScopeVersion) {
  return releaseRevisionReadSql(scope, ' FOR SHARE');
}

/** The single closed head shared by every owner-projection source read. Kept
 * here once so active-branch counts, Company setup reads and currency probes
 * emit byte-identical SQL from one owner-local definition; only columns,
 * projection, alias and predicate vary per caller. */
export function projectionSourceHeadSql(columns: string, projection: string, alias: string, predicate: string) {
  return fillReadHead(platformReadHeads.selectProjectionFrom, { columns, projection, alias, predicate });
}

/** The single active-branch count subquery shared by release acceptance guards,
 * inputs reads and the native acceptance predicate. Kept here once so the
 * "exactly one active branch" rule has one owner-local definition. */
export function activeBranchCountSql(companyExpression: string, options: {qualifier?: string; countAlias?: string} = {}) {
  const q = options.qualifier ? `${options.qualifier}.` : '';
  const count = options.countAlias ? `count(*) ${options.countAlias}` : 'count(*)';
  return projectionSourceHeadSql(count, organizationPlatformSources.organization_nodes, options.qualifier ?? 'organization_nodes', `${q}company_id=${companyExpression} AND ${q}kind='branch' AND ${q}status='active'`);
}

/** The single Company setup (business acceptance) projection read shared by
 * release acceptance guards and inputs reads. Kept here once so the accepted
 * setup rule has one owner-local definition. */
export function businessAcceptanceSetupSql(companyExpression: string, options: { columns?: string; acceptedVersion?: string } = {}) {
  const columns = options.columns ?? '1';
  const suffix = options.acceptedVersion ? ` AND status='accepted' AND version=${options.acceptedVersion}` : '';
  return projectionSourceHeadSql(columns, organizationPlatformSources.company_business_acceptance, 'company_business_acceptance', `company_id=${companyExpression}${suffix}`);
}

/** The single latest-acceptance subquery shared by release acceptance guards and
 * the native release predicate. It composes the parameterized row head with an
 * alias and ORDER BY/LIMIT suffix; the rendered SQL stays byte-identical. */
export function latestReleaseIdSql(companyExpression: string, scopeExpression: string) {
  return `(${releaseAcceptanceRowSql('latest.id', `latest.company_id=${companyExpression} AND latest.scope_version=${scopeExpression}`, ' ORDER BY latest.created_at DESC,latest.id DESC LIMIT 1', 'company_release_acceptances latest')})`;
}

/** The single parameterized row-read head shared by the latest-lookup, the base
 * evidence/finance-attachment reads and the revision read. The former acceptance
 * and revision heads were byte-identical apart from the table name, so the table
 * is now a parameter with the acceptance table as its default: every existing
 * caller renders exactly the same SQL, and no duplicate row-head is retained. */
export function releaseAcceptanceRowSql(columns: string, predicate: string, suffix = '', table = 'company_release_acceptances') {
  return fillReadHead(platformReadHeads.selectRowByPredicate, { columns, predicate, suffix, table });
}

/** The latest-acceptance row lookup composes the same byte-identical head with
 * its ORDER BY/LIMIT suffix. */
export function latestReleaseRowSql(columns: string, idPredicate: string) {
  return releaseAcceptanceRowSql(columns, `company_id=? ${idPredicate}`, ' ORDER BY created_at DESC,id DESC LIMIT 1');
}

/** The single THB-currency probe shared by every release-currency source. Kept
 * here once so this parameterized family has one owner-local definition; each
 * caller supplies only its closed source projection and company predicate. The
 * rendered SQL is byte-identical to the former inline copies. */
export function thbCurrencyProbeSql(source: string, condition: string) {
  return projectionSourceHeadSql('1', source, 'c', `c.company_id=scope_company.id AND ${condition}`);
}

export function releaseCurrencyPredicate(companyExpression: string) {
  // The scalar wrapper uses a parameter expression exactly once.
  return `(SELECT NOT EXISTS(${thbCurrencyProbeSql(financePlatformSources.finance_charts, "c.base_currency<>'THB'")})
    AND NOT EXISTS(${thbCurrencyProbeSql(commercePlatformSources.commerce_settings, "c.currency<>'THB'")})
    AND NOT EXISTS(${thbCurrencyProbeSql(posPlatformSources.pos_settings, "c.currency<>'THB'")})
    AND NOT EXISTS(${thbCurrencyProbeSql(pmsPlatformSources.pms_properties, "c.config_json::jsonb->>'currency' IS DISTINCT FROM 'THB'")})
    AND NOT EXISTS(${thbCurrencyProbeSql(people_payrollPlatformSources.hr_pay_groups, "c.status='active' AND (c.country_code<>'TH' OR c.currency<>'THB')")})
    FROM (SELECT ${companyExpression}::text AS id) scope_company)`;
}

export function acceptedReleasePredicate(companyExpression: string, scope: ReleaseScopeVersion = productionScope.version) {
  if (artifact.softwareVersion === 'unbuilt') return 'FALSE';
  const literal = (value: string) => `'${value.replaceAll("'", "''")}'`;
  const criteria = criteriaForScope(scope).map(row => `(${literal(row.id)},${literal(row.permission)})`).join(',');
  return `EXISTS(SELECT 1 FROM company_release_acceptances release
    JOIN ${releaseRevisionTable(scope)} revision ON revision.company_id=release.company_id
    JOIN (${organizationPlatformSources.company_business_acceptance}) setup ON setup.company_id=release.company_id
    WHERE release.company_id=${companyExpression} AND release.revoked_at IS NULL
    AND release.scope_version=${literal(scope)} AND release.software_version=${literal(artifact.softwareVersion)}
    AND release.configuration_revision=revision.revision AND setup.status='accepted' AND setup.version=release.setup_version
    AND release.id=${latestReleaseIdSql('release.company_id', 'release.scope_version')}
    ${scope === productionScope.version ? `AND (${activeBranchCountSql('release.company_id', { qualifier: 'branch' })})=1 AND ${releaseCurrencyPredicate('release.company_id')}` : ''}
    AND NOT EXISTS(${derivedSourceExistsSql(`VALUES ${criteria}`,'AS required(id,permission)',`NOT EXISTS(
      SELECT 1 FROM company_release_gate_reviews gate JOIN (${identity_accessPlatformSources.assignments}) reviewer ON reviewer.id=gate.assignment_id
      WHERE gate.company_id=release.company_id AND gate.release_id=release.id AND gate.criterion_id=required.id
      AND gate.state='passed' AND gate.permission=required.permission AND reviewer.user_id=gate.reviewer_id
      AND reviewer.company_id=release.company_id AND reviewer.scope_kind='holding'
      AND iam_assignment_active(reviewer.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)
      AND iam_principal_active(reviewer.user_id,reviewer.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)
      AND iam_role_allows(reviewer.role,reviewer.company_id,required.permission)
      AND NOT iam_same_person(gate.reviewer_id,release.requested_by,release.company_id))`)}))`;
}
