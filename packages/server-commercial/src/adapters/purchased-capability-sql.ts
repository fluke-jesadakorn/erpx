import "server-only";
import { commercialPackages, type CapabilityPurpose } from "@erpx/server-commercial/domain/packages";

const identifier = /^[a-z_][a-z_0-9]*(?:\.[a-z_][a-z_0-9]*)?$/;
const literal = (value: string) => `'${value.replaceAll("'", "''")}'`;
export const packageClockSql = "floor(extract(epoch FROM clock_timestamp())*1000)::bigint";

/** SQL equivalent of purchasedCapability, generated from the same versioned
 * definitions. Expressions must be code-owned identifiers, never request SQL.
 * Sample one clock in the caller's CTE for all candidates and dependencies. */
export function purchasedCapabilityPredicate(company: string, capability: string, purpose: CapabilityPurpose, now: string) {
  if (!identifier.test(company) || !identifier.test(now)) throw new Error("CAPABILITY_SQL_IDENTIFIER_REQUIRED");
  if (!["new-work", "settle-existing", "history"].includes(purpose)) throw new Error("CAPABILITY_PURPOSE_REQUIRED");
  const candidates = commercialPackages.filter(definition => [...definition.visibleCapabilities, ...definition.includedCapabilities].includes(capability));
  if (purpose === "history") {
    const sources = `SELECT package_id,package_version FROM company_package_grants WHERE company_id=${company} AND starts_at<=${now} AND state<>'inactive'
      UNION SELECT package_id,package_version FROM company_package_history_rights WHERE company_id=${company} AND retained_at<=${now}
        AND retained_at>=0 AND retained_at<=9007199254740991 AND source_grant_version>0 AND source_event_id<>''`;
    return candidates.length ? `(${candidates.map(definition => `(EXISTS(SELECT 1 FROM (${sources}) purchase WHERE purchase.package_id=${literal(definition.id)} AND purchase.package_version=${definition.version})${definition.dependencies.map(id => {
      const versions = commercialPackages.filter(parent => parent.id === id).map(parent => parent.version);
      return ` AND EXISTS(SELECT 1 FROM (${sources}) parent WHERE parent.package_id=${literal(id)} AND parent.package_version IN (${versions.join(",")}))`;
    }).join("")})`).join(" OR ")})` : "FALSE";
  }
  const available = (alias: string) => `${alias}.starts_at<=${now}${purpose === "new-work"
    ? ` AND ${alias}.state='active' AND (${alias}.ends_at IS NULL OR ${alias}.ends_at>${now})`
    : purpose === "settle-existing" ? ` AND ${alias}.state<>'inactive'` : ""}`;
  return candidates.length ? `(${candidates.map(definition => `EXISTS(SELECT 1 FROM company_package_grants purchase WHERE purchase.company_id=${company} AND purchase.package_id=${literal(definition.id)} AND purchase.package_version=${definition.version} AND ${available("purchase")}${definition.dependencies.map(id => {
    const versions = commercialPackages.filter(parent => parent.id === id).map(parent => parent.version);
    return ` AND EXISTS(SELECT 1 FROM company_package_grants parent WHERE parent.company_id=${company} AND parent.package_id=${literal(id)} AND parent.package_version IN (${versions.join(",")}) AND ${available("parent")})`;
  }).join("")})`).join(" OR ")})` : "FALSE";
}
