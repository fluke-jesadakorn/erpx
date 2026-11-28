import { finalRootOwnerProjectionSources as people_payrollProjectionSources } from "@erpx/people-payroll-server/public/final-root-owner-projection-sources";
import "server-only";
import { projectionLookup } from "./projection-lookup";
/** Canonical owner display-name image shared by the Work and Calendar read tails.
 * Callers pass only an owner-owned row alias; no caller SQL or relation is accepted. */
export const maintenanceOwnerName = (alias: string) =>
  projectionLookup("legal_name", people_payrollProjectionSources.hr_people, "hr_people", `company_id=${alias}.company_id AND user_id=${alias}.owner_id AND merged_into_id IS NULL LIMIT 1`);
