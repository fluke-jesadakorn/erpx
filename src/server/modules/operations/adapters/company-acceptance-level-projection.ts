import { finalRootOwnerProjectionSources as organizationProjectionSources } from "../../organization/public/final-root-owner-projection-sources";
export const companyAcceptanceLevelJoin=`LEFT JOIN (${organizationProjectionSources.staff_levels}) l ON l.id=e.level_id AND l.company_id=e.company_id`;
export const companyAcceptanceMissingLevel="l.id IS NULL OR l.active!=1";
