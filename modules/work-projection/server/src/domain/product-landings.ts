import {learningWorkerStartupEntry} from "@erpx/people-payroll-server/public/rules/learning-worker-startup-entry";
import {projectWorkerStartupEntry} from "@erpx/people-payroll-server/public/rules/project-worker-startup-entry";
import {servicePartsSetupEntry} from "@erpx/field-service-server/public/rules/parts-entry";
import {serviceInitialPhysicalStockCountEntry} from "@erpx/inventory-server/public/rules/service-initial-physical-stock-count";
import {socialPostSourceEntry} from '@erpx/collaboration-server/public/rules/product-entry';
import {assetsFinancialEntry} from '@erpx/finance-server/public/rules/assets-financial-entry';
import {workforceSelfServiceEntry} from '@erpx/people-payroll-server/public/rules/self-service-product-entry';
import { destinationAvailable } from "@erpx/work-projection-ui/lib/destination-access";
import {hasPermission} from '@erpx/identity-access-ui/lib/permissions';
import { destinations } from "@erpx/work-projection-ui/lib/destination-registry";
import { availableNavigationAppIds } from "@erpx/work-projection-ui/lib/workplace-catalogue";
import type { NavigationContext } from "@erpx/work-projection-ui/lib/workplace-navigation";
import { destinationProduct, product, type ProductId } from "@erpx/commercial-server/public/rules/product-registry";
import { productEntryCapability } from "@erpx/commercial-server/public/rules/product-entry-capability";

export type ProductLandingContext = Pick<NavigationContext, "role" | "permissions" | "scopeKind" | "beta"> & {
  modules: readonly string[];
  readyModules: readonly string[];
  capabilities: readonly string[];
  executiveVisible?: boolean;
  commercialAdmissionDeferred?: boolean;
};
function navigationContext(input: ProductLandingContext): NavigationContext {
  return { ...input, workspaceBase: "", moduleIds: input.modules,
    availableAppIds: availableNavigationAppIds(input.readyModules.map(id => ({ id, state: "Available", permitted: true }))) };
}

function feedAvailable(input: ProductLandingContext) {
  return input.readyModules.includes("collaboration") &&
    (input.commercialAdmissionDeferred || input.capabilities.includes("work.social"));
}

/** The same discovery predicates as owner menus. Owners still authorize the
 * actual page, record and command after entry; this is never a record grant. */
export function productDestinationAvailable(id: ProductId, path: string, input: ProductLandingContext) {
  if (destinationProduct(path) !== id) return false;
  if (id === "apps" && path === "home" && input.capabilities.includes("work.social")) return feedAvailable(input);
  // Financial continuation requires its named current-owner proof. The Assets
  // source menu also serves Maintenance roles and cannot discover this work.
  if (id === "assets" && assetsFinancialEntry(path)) return false;
  if (id === "learn" && learningWorkerStartupEntry(path)) return false;
  if (id === "projects" && projectWorkerStartupEntry(path)) return false;
  if (id === "service" && (servicePartsSetupEntry(path) || serviceInitialPhysicalStockCountEntry(path))) return false;
  const capability = productEntryCapability(id, path);
  if (!capability || !input.commercialAdmissionDeferred && !input.capabilities.includes(capability)) return false;
  // Standalone Routines are Operations-owned Company work, including the
  // original module-free staff/retained-history roles. Projects is an optional
  // source on a template, not the readiness owner for this Work collection.
  if(id==='apps'&&(path==='projects-routines/routines'||path.startsWith('projects-routines/routines/')))
    return /^projects-routines\/routines(?:\/(?!new(?:\/|$))[A-Za-z0-9_-]+(?:\/overview)?)?$/.test(path)&&
      !input.beta&&input.readyModules.includes('operations')&&hasPermission({role:input.role??'employee',permissions:input.permissions},'records.read');
  if(id==="apps"&&socialPostSourceEntry(path))return input.readyModules.includes("collaboration");
  if(id==="apps"&&workforceSelfServiceEntry(path))return input.readyModules.includes("people_payroll");
  const context = navigationContext(input);
  const allowed = (item: typeof destinations[number]) => {
    const required = productEntryCapability(id, item.path);
    return !!required && (input.commercialAdmissionDeferred || input.capabilities.includes(required)) &&
      !(input.scopeKind === "department" && item.source === "commerce" && item.selection.view === "stock") && destinationAvailable(context, item);
  };
  // Profile links omit the "current" segment used by the directory list.
  const currentPerson = id === "people"
    && /^people-organization\/people\/directory\/person\/[^/]+(?:\/(?:profile|employment|compensation|history|working-time))?$/.test(path);
  const collectionPath = currentPerson ? "people-organization/people/directory/current" : path;
  const exact = destinations.find(item => item.path === collectionPath);
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
  if (id === "apps") {
    if (feedAvailable(input)) return definition.landing;
    if (input.capabilities.includes("work.social") && !input.commercialAdmissionDeferred) return null;
    return input.readyModules.includes("operations") &&
      (input.commercialAdmissionDeferred || input.capabilities.includes("work.today")) ? definition.landing : null;
  }
  if (!definition.modules.some(moduleId => input.modules.includes(moduleId) && input.readyModules.includes(moduleId))) return null;
  if (productDestinationAvailable(id, definition.landing, input)) return definition.landing;
  return destinations.find(item => destinationProduct(item.path) === id && productDestinationAvailable(id, item.path, input))?.path ?? null;
}
