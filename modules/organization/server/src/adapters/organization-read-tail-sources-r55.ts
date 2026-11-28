import { platformOwnerProjectionSources as people_payrollPlatformSources } from "@erpx/people-payroll-server/public/platform-owner-projection-sources";

/** Byte-identical latest active employee-engagement point read. The owner
 * supplies only the extra predicate; the caller keeps its own bind list and
 * supplied transaction. */
export const latestEmployeeEngagementId = (extra: string) =>
  `(SELECT x.id FROM (${people_payrollPlatformSources.hr_engagements}) x WHERE x.person_id=p.id AND x.company_id=p.company_id ${extra}ORDER BY CASE WHEN x.status IN ('active','onboarding','suspended') THEN 0 ELSE 1 END,x.start_date DESC,x.id LIMIT 1)`;

/** Byte-identical person/engagement existence head. The owner supplies only
 * the tail, including its placeholder style, binds and lock-free closure. */
export const personEngagementJoin = (tail: string) =>
  `SELECT 1 FROM (${people_payrollPlatformSources.hr_people}) p JOIN (${people_payrollPlatformSources.hr_engagements}) e ON e.person_id=p.id AND e.company_id=p.company_id ${tail}`;
