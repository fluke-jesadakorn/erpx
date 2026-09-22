import { redirect } from "next/navigation";

export default async function LegacyLedger({params}:{params:Promise<{holdingSlug:string}>}) {
  redirect(`/holdings/${encodeURIComponent((await params).holdingSlug)}/finance/accounting`);
}
