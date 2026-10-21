import { commercePurchaseControls } from "@/server/modules/commerce/public/purchase-controls";
import { CommerceEnvironment } from "@/components/commerce/purchase-context";
import { canonicalUiHref } from "@/lib/work_projection/destination-registry";
import Link from "@/components/workplace/workspace-link";
import { FeaturePage } from "@/components/shared/features/feature-page";
import { FinanceWorkspace } from "@/components/finance/finance-workspace";
import { SalesInvoiceEditor } from "@/components/finance/sales-controls";
import { OrderEditor } from "@/components/commerce/commerce-controls";
import { requireWorkContext, publicScope } from "@/server/modules/identity_access/public/authorization";
import { requirePermission } from "@/server/modules/identity_access/public/iam-permissions";
import { salesDefaultsWorkspace } from "@/server/modules/finance/public/sales";
import { financePurchaseControls } from "@/server/modules/finance/public/purchases";
import { financeSalesInvoiceNewWorkAllowed } from "@/server/modules/finance/public/rules/purchase-controls";
import { commerceOverview } from "@/server/modules/commerce/public/commerce";
import { workspaceBase } from "@/lib/work_projection/workspace";
import { firstPageSearchValues, type PageSearchParams } from "@/lib/shared/page-search-params";
import { ApiError } from "@/server/modules/runtime/public/http";
import { pageError } from "@/server/modules/work_projection/public/page-error";
import { redirect } from "next/navigation";
export const dynamic = "force-dynamic";
export default async function NewSale({ params, searchParams }: { params: Promise<{holdingSlug:string}>; searchParams: Promise<PageSearchParams> }) {
  const {holdingSlug}=await params,actor = await requireWorkContext(holdingSlug), base = workspaceBase(publicScope(actor)), {kind} = firstPageSearchValues(await searchParams);
  const invoice = kind === "invoice"; requirePermission(actor, invoice ? "finance.write" : "commerce.sales");
  const invoicePurchases = invoice ? await financePurchaseControls(actor,["finance.accounting","finance.sales-posting"]).catch(error=>pageError(error,holdingSlug)) : undefined;
  const invoiceNewWorkAllowed = !invoice || financeSalesInvoiceNewWorkAllowed(invoicePurchases);
  if (invoice && !invoiceNewWorkAllowed) redirect(canonicalUiHref(`${base}/finance/sales?tab=invoices`));
  const setup = invoice ? await salesDefaultsWorkspace(actor).catch(error=>pageError(error,holdingSlug)) : null;
  let options = null;
  if (!invoice) try { options = await commerceOverview(actor); } catch (error) { if (!(error instanceof ApiError && error.code === "COMMERCE_SETUP_REQUIRED")) return pageError(error, actor.holdingSlug); }
  const purchase = !invoice ? await commercePurchaseControls(actor,{type:"kind",kind:kind === "order" ? "sales" : "quote"}) : undefined;
  const approved = setup?.editions.filter(row => setup.usableDefaults.includes(row.id)) ?? [];
  return <FeaturePage className="ui-page-content"><FinanceWorkspace base={base} active="finance-sales" title={invoice ? "Invoice completed services" : kind === "order" ? "New sales order" : "New quotation"} description="Prepare a document for review."><Link href={canonicalUiHref(`${base}/finance/sales`)}>Back to Sales & collections</Link>{setup && (approved.length ? <SalesInvoiceEditor inline base={base} parties={setup.options.parties} defaults={approved} /> : <p>Approve sales defaults first. <Link href={canonicalUiHref(`${base}/finance/sales/settings`)}>Open sales settings</Link></p>)}{!invoice && !options && <p>Approve Company sales defaults before preparing documents. <Link href={canonicalUiHref(`${base}/finance/sales/settings`)}>Open sales settings</Link> · <Link href={canonicalUiHref(`${base}/finance/setup`)}>Create ledgers and accounts</Link></p>}{options && <CommerceEnvironment purchase={purchase}><OrderEditor base={base} options={options} kind={kind === "order" ? "sales" : "quote"} initialOpen /></CommerceEnvironment>}</FinanceWorkspace></FeaturePage>;
}
