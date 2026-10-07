import { platformOwnerProjectionSources as identity_accessPlatformSources } from "../../identity_access/public/platform-owner-projection-sources";
import { platformOwnerProjectionSources as people_payrollPlatformSources } from "../../people_payroll/public/platform-owner-projection-sources";
import 'server-only';
import type { PoolClient, Pool } from 'pg';
import type { OperationTransaction } from '@/server/modules/identity_access/public/directory-operation-contract';
export async function affectedPositions(c: Pick<PoolClient | Pool, 'query'>, facts: Parameters<OperationTransaction['affectedPositions']>[0]): ReturnType<OperationTransaction['affectedPositions']> {
    const { relatedConnections, ids } = facts;
    return (await c.query(`SELECT DISTINCT p.id,p.company_id,p.title FROM organization_positions p JOIN (${people_payrollPlatformSources.people_position_packages}) b ON b.position_id=p.id AND b.company_id=p.company_id JOIN (${people_payrollPlatformSources.people_package_revisions}) r ON r.package_id=b.package_id AND r.revision=b.revision AND r.company_id=b.company_id WHERE r.definition->>'connectionId'=ANY($1::text[]) AND (EXISTS(SELECT 1 FROM jsonb_array_elements_text(r.definition->'directoryGroupIds') g WHERE g=ANY($2::text[])) OR EXISTS(SELECT 1 FROM jsonb_array_elements(r.definition->'windowsPolicies') w WHERE w->>'id'=ANY($2::text[]))) ORDER BY p.title`, [relatedConnections.map((r) => r.id), ids])).rows;
}
export async function companyNames(c: Pick<PoolClient | Pool, 'query'>, facts: Parameters<OperationTransaction['companyNames']>[0]): ReturnType<OperationTransaction['companyNames']> {
    const { companyIds } = facts;
    return (await c.query("SELECT company_id id,display_name name FROM organization_nodes WHERE company_id=ANY($1::text[]) AND kind='company' ORDER BY company_id", [companyIds])).rows;
}
export async function membershipSources(c: Pick<PoolClient | Pool, 'query'>, facts: Parameters<OperationTransaction['membershipSources']>[0]): ReturnType<OperationTransaction['membershipSources']> {
    const { conn, action } = facts;
    return (await c.query(`SELECT s.member_id,s.application_id,
              (s.application_id IS NOT NULL AND ap.ended_at IS NULL) package_owned,
              EXISTS(SELECT 1 FROM (${identity_accessPlatformSources.organization_gpo_membership_claims}) g WHERE
                g.connection_id=s.connection_id AND g.group_id=s.group_id
                AND g.member_id=s.member_id AND g.source_id=s.source_id) gpo_owned
             FROM (${identity_accessPlatformSources.people_directory_membership_sources}) s LEFT JOIN (${people_payrollPlatformSources.people_applied_packages}) ap ON ap.id=s.application_id
             WHERE s.connection_id=$1 AND s.group_id=$2 AND s.member_id=ANY($3::text[])`, [conn.id, action.objectId, action.memberIds])).rows;
}
