import Screen from "@/app/holdings/[holdingSlug]/_presenters/finance/invoices/screen";

export default async function Page({
  params,
  searchParams,
}: {
  params: Promise<{ holdingSlug: string }>;
  searchParams: Promise<Record<string, string | string[] | undefined>>;
}) {
  return Screen({ params, searchParams } as Parameters<typeof Screen>[0]);
}
