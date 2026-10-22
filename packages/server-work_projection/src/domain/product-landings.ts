import {learningWorkerStartupEntry} from "@erpx/server-people_payroll/public/rules/learning-worker-startup-entry";
import {projectWorkerStartupEntry} from "@erpx/server-people_payroll/public/rules/project-worker-startup-entry";
import {servicePartsSetupEntry} from "@erpx/server-field_service/public/rules/parts-entry";
import {serviceInitialPhysicalStockCountEntry} from "@erpx/server-inventory/public/rules/service-initial-physical-stock-count";
import {socialPostSourceEntry} from '@erpx/server-collaboration/public/rules/product-entry';
import {assetsFinancialEntry} from '@erpx/server-finance/public/rules/assets-financial-entry';
import {workforceSelfServiceEntry} from '@erpx/server-people_payroll/public/rules/self-service-product-entry';
import { destinationAvailable } from "@erpx/feature-work_projection/lib/destination-access";
import {hasPermission} from '@erpx/feature-identity_access/lib/permissions';
import { destinations } from "@erpx/feature-work_projection/lib/destination-registry";
import { availableNavigationAppIds } from "@erpx/feature-work_projection/lib/workplace-catalogue";
import type { NavigationContext } from "@erpx/feature-work_projection/lib/workplace-navigation";
import { destinationProduct, product, type ProductId } from "@erpx/server-commercial/public/rules/product-registry";
import { productEntryCapability } from "@erpx/server-commercial/public/rules/product-entry-capability";

export type ProductLandingContext = Pick<NavigationContext, "role" | "permissions" | "scopeKind" | "beta"> & {
  modules: readonly string[];
  readyModules: readonly string[];
  capabilities: readonly string[];
  executiveVisible?: boolean;
};
function navigationContext(input: ProductLandingContext): NavigationContext {
  return { ...input, workspaceBase: "", moduleIds: input.modules,
    availableAppIds: availableNavigationAppIds(input.readyModules.map(id => ({ id, state: "Available", permitted: true }))) };
}

/** The same discovery predicates as owner menus. Owners still authorize the
 * actual page, record and command after entry; this is never a record grant. */
export function productDestinationAvailable(id: ProductId, path: string, input: ProductLandingContext) {
  if (destinationProduct(path) !== id) return false;
  // Financial continuation requires its named current-owner proof. The Assets
  // source menu also serves Maintenance roles and cannot discover this work.
  if (id === "assets" && assetsFinancialEntry(path)) return false;
  if (id === "learn" && learningWorkerStartupEntry(path)) return false;
  if (id === "projects" && projectWorkerStartupEntry(path)) return false;
  if (id === "service" && (servicePartsSetupEntry(path) || serviceInitialPhysicalStockCountEntry(path))) return false;
  const capability = productEntryCapability(id, path);
  if (!capability || !input.capabilities.includes(capability)) return false;
  // Standalone Routines are Operations-owned Company work, including the
  // original module-free staff/retained-history roles. Projects is an optional
  // source on a template, not the readiness owner for this Work collection.
  if(id==='work'&&(path==='projects-routines/routines'||path.startsWith('projects-routines/routines/')))
    return /^projects-routines\/routines(?:\/(?!new(?:\/|$))[A-Za-z0-9_-]+(?:\/overview)?)?$/.test(path)&&
      !input.beta&&input.readyModules.includes('operations')&&hasPermission({role:input.role??'employee',permissions:input.permissions},'records.read');
  if(id==="work"&&socialPostSourceEntry(path))return input.readyModules.includes("collaboration");
  if(id==="work"&&workforceSelfServiceEntry(path))return input.readyModules.includes("people_payroll");
  const context = navigationContext(input);
  const allowed = (item: typeof destinations[number]) => {
    const required = productEntryCapability(id, item.path);
    return !!required && input.capabilities.includes(required) &&
      !(input.scopeKind === "department" && item.source === "commerce" && item.selection.view === "stock") && destinationAvailable(context, item);
  };
  const exact = destinations.find(item => item.path === path);
  if (exact) return allowed(exact);
  // Section indexes render only their permitted registered children.
  const children = destinations.filter(item => item.path.startsWith(path + "/") && destinationProduct(item.path) === id);
  if (children.length) return children.some(allowed);
  // Deep links retain their owner record policy. A role must at least be able
  // to open the source collection; no arbitrary product-prefix fallback.
  const collection = destinations.filter(item => path.startsWith(item.path + "/") && destinationProduct(item.path) === id)
    .sort((a, b) => b.path.length - a.path.length)[0];
  return !!collection && allowed(collection);
}

export function productOfficeLanding(id: ProductId, input: ProductLandingContext) {
  const definition = product(id);
  if (!definition) return null;
  if (id === "work") return input.readyModules.includes("operations") && input.capabilities.includes("work.today") ? definition.landing : null;
  if (!definition.modules.some(moduleId => input.modules.includes(moduleId) && input.readyModules.includes(moduleId))) return null;
  if (productDestinationAvailable(id, definition.landing, input)) return definition.landing;
  return destinations.find(item => destinationProduct(item.path) === id && productDestinationAvailable(id, item.path, input))?.path ?? null;
}
