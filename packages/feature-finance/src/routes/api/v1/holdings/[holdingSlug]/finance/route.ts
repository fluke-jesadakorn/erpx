import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import { requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { errorResponse } from "@erpx/server-runtime/public/http";
import { listCharts, listInvoices, listJournals, listPayments, statements } from "@erpx/server-finance/public/finance";

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
    return Response.json(canonicalDestinationLinks({ data: { charts, journals, invoices, payments, report } }));
  } catch (error) {
    return errorResponse(error);
  }
}
