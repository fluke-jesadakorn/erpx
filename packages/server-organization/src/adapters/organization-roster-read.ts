import 'server-only';
import type{SqlDatabase}from '@erpx/server-runtime/public/runtime/types';
import type{OrganizationPerson,OrganizationPosition}from '@erpx/feature-organization/lib/organization';
import{policyRead}from '@erpx/server-identity_access/public/iam-read-snapshot';
import{nativeOrgAll}from './native-organization-owner-write';
export async function organizationRoster(db: SqlDatabase, companyId: string) {
  return structuredClone(await policyRead(db,JSON.stringify(["organization-roster",companyId]),async () => {
  const [people, positions] = await Promise.all([
    nativeOrgAll<OrganizationPerson>(db,companyId,'org_readrosterpeople',[companyId]),
    nativeOrgAll<OrganizationPosition>(db,companyId,'org_readrosterpositions',[companyId]),
  ]);
  return {people,positions};
  }));
}
