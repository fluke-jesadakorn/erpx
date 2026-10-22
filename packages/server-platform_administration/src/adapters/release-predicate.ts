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

export function releaseRevisionTable(scope: ReleaseScopeVersion) {
  return scope === legalProductionScope.version ? 'company_legal_release_revisions' : 'company_release_revisions';
}

export function releaseCurrencyPredicate(companyExpression: string) {
  // The scalar wrapper uses a parameter expression exactly once.
  return `(SELECT NOT EXISTS(SELECT 1 FROM (${financePlatformSources.finance_charts}) c WHERE c.company_id=scope_company.id AND c.base_currency<>'THB')
    AND NOT EXISTS(SELECT 1 FROM (${commercePlatformSources.commerce_settings}) c WHERE c.company_id=scope_company.id AND c.currency<>'THB')
    AND NOT EXISTS(SELECT 1 FROM (${posPlatformSources.pos_settings}) c WHERE c.company_id=scope_company.id AND c.currency<>'THB')
    AND NOT EXISTS(SELECT 1 FROM (${pmsPlatformSources.pms_properties}) c WHERE c.company_id=scope_company.id AND c.config_json::jsonb->>'currency' IS DISTINCT FROM 'THB')
    AND NOT EXISTS(SELECT 1 FROM (${people_payrollPlatformSources.hr_pay_groups}) c WHERE c.company_id=scope_company.id AND c.status='active' AND (c.country_code<>'TH' OR c.currency<>'THB'))
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
    AND release.id=(SELECT latest.id FROM company_release_acceptances latest WHERE latest.company_id=release.company_id AND latest.scope_version=release.scope_version ORDER BY latest.created_at DESC,latest.id DESC LIMIT 1)
    ${scope === productionScope.version ? `AND (SELECT count(*) FROM (${organizationPlatformSources.organization_nodes}) branch WHERE branch.company_id=release.company_id AND branch.kind='branch' AND branch.status='active')=1 AND ${releaseCurrencyPredicate('release.company_id')}` : ''}
    AND NOT EXISTS(SELECT 1 FROM (VALUES ${criteria}) AS required(id,permission) WHERE NOT EXISTS(
      SELECT 1 FROM company_release_gate_reviews gate JOIN (${identity_accessPlatformSources.assignments}) reviewer ON reviewer.id=gate.assignment_id
      WHERE gate.company_id=release.company_id AND gate.release_id=release.id AND gate.criterion_id=required.id
      AND gate.state='passed' AND gate.permission=required.permission AND reviewer.user_id=gate.reviewer_id
      AND reviewer.company_id=release.company_id AND reviewer.scope_kind='holding'
      AND iam_assignment_active(reviewer.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)
      AND iam_principal_active(reviewer.user_id,reviewer.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)
      AND iam_role_allows(reviewer.role,reviewer.company_id,required.permission)
      AND NOT iam_same_person(gate.reviewer_id,release.requested_by,release.company_id))))`;
}
