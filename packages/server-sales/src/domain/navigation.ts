import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import { hasPermission } from "@erpx/feature-identity_access/lib/permissions";
import type { PermissionActor } from "@erpx/feature-identity_access/lib/iam";
import { salesCustomerAccessPolicy, salesNavigationRoutes } from "@erpx/contracts/sales-navigation";
export { salesViews, customersViews, customerReviews, queryValue } from "@erpx/contracts/sales-navigation";
export type { HubQuery, SalesView, CustomersView, CustomerReview } from "@erpx/contracts/sales-navigation";
export const { hubHref, salesCustomerLegacyDestination } = salesNavigationRoutes(canonicalUiHref);
export function salesCustomerAccess(actor: PermissionActor & { branchId?: string | null; departmentId?: string | null }, apps?: readonly string[]) {
  return salesCustomerAccessPolicy({commerceRead: hasPermission(actor, "commerce.read"), commerceSales: hasPermission(actor, "commerce.sales"), supportRead: hasPermission(actor, "support.read"), recordsRead: hasPermission(actor, "records.read")}, actor, apps);
}
