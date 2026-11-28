import { notFound } from "next/navigation";
import { recordSections, defaultRecordSection } from "@erpx/feature-records/lib/record-drawer";
import { InvoiceBackdrop } from "../invoice-backdrop";

export default async function Page({ params, searchParams }: {
  params: Promise<{ holdingSlug: string; id: string; recordSection: string }>;
  searchParams: Promise<Record<string, string | string[] | undefined>>;
}) {
  const route = await params;
  if (!(recordSections.invoice ?? [defaultRecordSection("invoice")]).includes(route.recordSection)) notFound();
  return <InvoiceBackdrop params={route} searchParams={searchParams} />;
}
