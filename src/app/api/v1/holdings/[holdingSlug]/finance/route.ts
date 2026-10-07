import { requireWorkContext } from "@/server/modules/identity_access/public/authorization";
import { errorResponse } from "@/server/modules/runtime/public/http";
import { listCharts, listInvoices, listJournals, listPayments, statements } from "@/server/modules/finance/public/finance";

export const dynamic = "force-dynamic";

export async function GET(request: Request, { params }: { params: Promise<{ holdingSlug: string }> }) {
  try {
    const { holdingSlug } = await params;
    const actor = await requireWorkContext(holdingSlug);
    const charts=await listCharts(actor);const chartId=new URL(request.url).searchParams.get("chartId")??String(charts.find(c=>c.name==="Operations ledger")?.id??charts[0]?.id??"");
    const [journals, invoices, payments, report] = await Promise.all([
      listJournals(actor, {}),
      listInvoices(actor),
      listPayments(actor),
      statements(actor,undefined,chartId),
    ]);
    return Response.json({ data: { charts, journals, invoices, payments, report } });
  } catch (error) {
    return errorResponse(error);
  }
}
