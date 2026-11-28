import { headers } from "next/headers";
import { FinanceSourceDocumentBackdrop } from "@erpx/feature-finance/components/source-document-backdrop";
import { invoiceDrawerListSearch } from "@erpx/feature-finance/lib/invoice-drawer-list-search";
import InvoiceList from "@erpx/workplace/routes/holdings/[holdingSlug]/_presenters/finance/invoices/screen";
import SalesList from "@erpx/workplace/routes/holdings/[holdingSlug]/_presenters/finance/sales/screen";

export async function InvoiceBackdrop({
  params,
  searchParams,
}: {
  params: { holdingSlug: string; id: string };
  searchParams: Promise<Record<string, string | string[] | undefined>>;
}) {
  const { holdingSlug, id } = params;
  const source = await FinanceSourceDocumentBackdrop({ holdingSlug, kind: "invoice", id });
  if (source) return source;

  const path = (await headers()).get("x-erpx-page-path");
  const pathname = path ? new URL(path, "https://entry.invalid").pathname : "";
  const base = pathname.match(/^(.*?)\/finance\/documents\/invoices\//)?.[1] ?? `/holdings/${holdingSlug}`;
  const query = invoiceDrawerListSearch(base, await searchParams);
  const props = { params: Promise.resolve(params), searchParams: Promise.resolve(query) };
  return query.tab === "invoices" ? <SalesList {...props} /> : <InvoiceList {...props} />;
}
