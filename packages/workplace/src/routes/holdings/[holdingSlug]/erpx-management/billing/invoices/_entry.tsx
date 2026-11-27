import { notFound } from "next/navigation";
import { BillingConsole } from "@erpx/feature-billing/components/billing-console";
import { PlatformInvoicePrint } from "@erpx/feature-billing/components/platform-invoice-print";
import type { InvoiceLine } from "@erpx/feature-billing/lib/billing";
import { platformBillingPresentation } from "@erpx/server-billing/public/billing-presentation";

export default async function Page({ searchParams }: {
  searchParams: Promise<Record<string, string | string[] | undefined>>;
}) {
  const query = await searchParams;
  const initial = await platformBillingPresentation(typeof query.holdingId === "string" ? query.holdingId : undefined);
  if (query.printInvoice !== undefined) {
    const invoice = initial.data?.invoices.find(item => item.id === query.printInvoice);
    const customer = initial.accounts.find(item => item.holding_id === initial.holdingId);
    if (typeof query.holdingId !== "string" || !invoice?.stripe_id || !customer || initial.data?.sandbox !== true) notFound();
    const lines: InvoiceLine[] = JSON.parse(invoice.lines_json);
    return <PlatformInvoicePrint customer={customer.display_name} invoice={invoice} lines={lines} />;
  }
  return <BillingConsole platform writable={initial.writable} initialAccounts={initial.accounts} initialHolding={initial.holdingId} initialData={initial.data} />;
}
