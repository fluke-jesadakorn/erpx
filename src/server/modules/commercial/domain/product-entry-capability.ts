import {learningWorkerStartupEntry} from "../../people_payroll/public/rules/learning-worker-startup-entry";
import {projectWorkerStartupEntry} from "../../people_payroll/public/rules/project-worker-startup-entry";
import {manufacturingInitialPhysicalStockCountEntry} from "../../inventory/public/rules/manufacturing-initial-physical-stock-count";
import {socialPostSourceEntry} from '../../collaboration/public/rules/product-entry';
import {workforceSelfServiceEntry} from '../../people_payroll/public/rules/self-service-product-entry';
import {peopleFinancialEntry} from '../../people_payroll/public/rules/financial-product-entry';
import {supportTicketPortalEntry,supportTicketPortalLanding} from '../../support/public/rules/ticket-portal-entry';
import { destinationProduct, type ProductId } from "@/lib/commercial/product-registry";

/** Entry/collection discovery only. Record panels and commands retain their
 * owner-specific capability and record-policy checks. */
export function productEntryCapability(id: ProductId, path: string): string | null {
  if (destinationProduct(path) !== id) return null;
  switch (id) {
    case "work": return socialPostSourceEntry(path)?"work.social":workforceSelfServiceEntry(path)?"people.workforce":path.startsWith("projects-routines/routines") || path.startsWith("projects-routines/operations") ? "work.advanced-routines"
      : path.startsWith("inbox/messages") || path.startsWith("inbox/teams") ? "work.messages"
        : path.startsWith("calendar") ? "work.calendar" : path.startsWith("knowledge") ? "work.handbook"
          : path.startsWith("work") || path.startsWith("projects-routines/tasks") ? "work.actions" : "work.today";
    case "finance": return peopleFinancialEntry(path)?.kind === "expense" ? "people.expenses"
      : /^(?:finance\/accounting\/(?:engines|instruments|book-adjustments)|finance\/reports\/(?:group|group-editions))(?:\/|$)/.test(path)
        ? "finance.advanced" : "finance.accounting";
    case "people": return path.startsWith("people-organization/benefits") ? "people.benefits"
      : /\/people\/(?:payroll|payslips)(?:\/|$)/.test(path) ? "people.payroll"
        : path.includes("/people/roster") ? "people.workforce" : path.includes("/people/evaluation") ? "people.performance" : "people.directory";
    case "sales": return path.startsWith("customers-sales/marketing") ? "sales.marketing"
      : path.startsWith("customers-sales/ecommerce") ? "sales.ecommerce"
        : path.startsWith("customers-sales/customers/reviews/access") ? "sales.portal"
          : path.startsWith("customers-sales/customers") ? "sales.customers"
            : path.startsWith("customers-sales/sales/pipeline") || path.startsWith("customers-sales/sales/reports") ? "sales.crm"
              : path.startsWith("customers-sales/sales/documents/quote") ? "sales.quotes" : "sales.orders";
    case "supply": return path.startsWith("inventory-quality/quality") ? "supply.quality"
      : path.startsWith("purchasing/landed-costs") ? "supply.advanced-inventory"
        : path.startsWith("purchasing") ? "supply.purchasing" : path.startsWith("inventory-quality/transfers") || path.startsWith("inventory-quality/transit") ? "supply.transfers"
          : path.startsWith("inventory-quality/counts") ? "supply.counts" : "supply.stock";
    case "assets": return path.startsWith("services/assets/plans") ? "assets.preventive"
      : /^services\/assets\/maintenance\/[^/]+\/(?:parts|costs)(?:\/|$)/.test(path) ? "assets.parts-costs"
        : path.startsWith("services/assets/maintenance") ? "assets.maintenance" : "assets.register";
    case "projects": return projectWorkerStartupEntry(path) ? "people.worker-reference" : /\/projects\/[^/]+\/(?:costs|costing)(?:\/|$)/.test(path) ? "projects.costs" : "projects.wbs";
    case "recruit": return "recruit.positions";
    case "learn": return learningWorkerStartupEntry(path) ? "people.learner-reference" : "learn.assignments";
    case "support": return supportTicketPortalEntry(path) ? "support.portal"
      : path.startsWith(supportTicketPortalLanding + "/") ? null : "support.tickets";
    case "service": return path === "services/field-service/settings/parts" || path === "services/field-service/settings/initial-stock" || path.startsWith("services/field-service/settings/initial-stock/") ? "service.parts-assets" : "service.orders";
    case "legal": return "legal.matters";
    case "manufacturing": return manufacturingInitialPhysicalStockCountEntry(path) ? "inventory.stock-core" : path === "production/manufacturing/settings/posting" ? "manufacturing.costs" : path.startsWith("production/planning/mrp") ? "planning.advanced-mrp"
      : path.startsWith("production/planning") ? "planning.basic" : "manufacturing.execution";
    case "pos": return "pos.sales";
    case "pms": return /^pms\/minibar(?:\/|$)/.test(path) ? "pms.minibar" : "pms.front-desk";
  }
}
