import { InvoiceBackdrop } from "./invoice-backdrop";

export default async function Page({ params, searchParams }: {
  params: Promise<{ holdingSlug: string; id: string }>;
  searchParams: Promise<Record<string, string | string[] | undefined>>;
}) {
  return <InvoiceBackdrop params={await params} searchParams={searchParams} />;
}
