import "server-only";
import { runtimeEnv } from "../../../../packages/server-runtime/src/public/runtime";
import { ApiError } from "@erpx/server-runtime/public/http";

export type PackageEnforcementMode = "disabled" | "shadow" | "enforced";
/** Development-only rollout, independently reversible from routing and SSO.
 * Never enable implicitly from a purchased grant or a product-host request. */
async function developmentPackageEnforcementMode(key: "ERPX_WORK_PACKAGE_ENFORCEMENT" | "ERPX_SUPPORT_PACKAGE_ENFORCEMENT" | "ERPX_SERVICE_PACKAGE_ENFORCEMENT" | "ERPX_POS_PACKAGE_ENFORCEMENT" | "ERPX_PMS_PACKAGE_ENFORCEMENT" | "ERPX_FINANCE_PACKAGE_ENFORCEMENT" | "ERPX_PEOPLE_PACKAGE_ENFORCEMENT" | "ERPX_COMMERCE_PACKAGE_ENFORCEMENT" | "ERPX_PROJECTS_PACKAGE_ENFORCEMENT" | "ERPX_ASSETS_PACKAGE_ENFORCEMENT" | "ERPX_LEARNING_PACKAGE_ENFORCEMENT" | "ERPX_MANUFACTURING_PACKAGE_ENFORCEMENT" | "ERPX_TALENT_PACKAGE_ENFORCEMENT" | "ERPX_LEGAL_PACKAGE_ENFORCEMENT"): Promise<PackageEnforcementMode> {
  const mode = process.env[key] ?? "disabled";
  if (!["disabled", "shadow", "enforced"].includes(mode)) throw new ApiError(503, "PACKAGE_ENFORCEMENT_CONFIGURATION", "Package enforcement is unavailable.");
  if (mode !== "disabled" && (await runtimeEnv()).ERPX_ENVIRONMENT !== "local")
    throw new ApiError(503, "PACKAGE_ENFORCEMENT_ENVIRONMENT", "Package enforcement is unavailable in this environment.");
  return mode as PackageEnforcementMode;
}

export const supportPackageEnforcementMode = () => developmentPackageEnforcementMode("ERPX_SUPPORT_PACKAGE_ENFORCEMENT");
export const servicePackageEnforcementMode = () => developmentPackageEnforcementMode("ERPX_SERVICE_PACKAGE_ENFORCEMENT");
export const posPackageEnforcementMode = () => developmentPackageEnforcementMode("ERPX_POS_PACKAGE_ENFORCEMENT");
export const pmsPackageEnforcementMode = () => developmentPackageEnforcementMode("ERPX_PMS_PACKAGE_ENFORCEMENT");
export const financePackageEnforcementMode = () => developmentPackageEnforcementMode("ERPX_FINANCE_PACKAGE_ENFORCEMENT");
export const peoplePackageEnforcementMode = () => developmentPackageEnforcementMode("ERPX_PEOPLE_PACKAGE_ENFORCEMENT");

export const commercePackageEnforcementMode = () => developmentPackageEnforcementMode("ERPX_COMMERCE_PACKAGE_ENFORCEMENT");
export const projectsPackageEnforcementMode = () => developmentPackageEnforcementMode("ERPX_PROJECTS_PACKAGE_ENFORCEMENT");
export const assetsPackageEnforcementMode = () => developmentPackageEnforcementMode("ERPX_ASSETS_PACKAGE_ENFORCEMENT");
export const learningPackageEnforcementMode = () => developmentPackageEnforcementMode("ERPX_LEARNING_PACKAGE_ENFORCEMENT");
export const manufacturingPackageEnforcementMode = () => developmentPackageEnforcementMode("ERPX_MANUFACTURING_PACKAGE_ENFORCEMENT");
export const talentPackageEnforcementMode = () => developmentPackageEnforcementMode("ERPX_TALENT_PACKAGE_ENFORCEMENT");
export const legalPackageEnforcementMode = () => developmentPackageEnforcementMode("ERPX_LEGAL_PACKAGE_ENFORCEMENT");

export const workPackageEnforcementMode = () => developmentPackageEnforcementMode("ERPX_WORK_PACKAGE_ENFORCEMENT");
