/** Byte-identical company_provisioning point-read family shared by the attachment-owner fact unit.
 * Callers pass only trusted literals: a fixed projection and an optional trailing predicate.
 * Every caller keeps its own binds and statement; no request text is interpolated. */
export type CompanyProvisioningPointProjection =
  | 'company_id,NULL::text branch_id,NULL::text department_id,version'
  | '1';
export const companyProvisioningPointSql = (
  projection: CompanyProvisioningPointProjection,
  tail = '',
) => `SELECT ${projection} FROM company_provisioning WHERE company_id=? AND company_id=?${tail}`;
