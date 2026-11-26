import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import { requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { ApiError, errorResponse } from "@erpx/server-runtime/public/http";
import {financeChartListNative,financeInvoiceListNative,listPayments,statements} from '@erpx/server-finance/public/finance';
import {nativeJournalList} from '@erpx/feature-finance/lib/native-journal-list';

export const dynamic = "force-dynamic";

export async function GET(request: Request, { params }: { params: Promise<{ holdingSlug: string }> }) {
  try {
    const { holdingSlug } = await params;
    const actor = await requireWorkContext(holdingSlug);
    const native = await nativeJournalList(actor, holdingSlug, {});
    if (!native) throw new ApiError(503, "NATIVE_FINANCE_UNAVAILABLE", "The native Finance journal-list service is unavailable.");
    const charts=await financeChartListNative(actor);const chartId=new URL(request.url).searchParams.get("chartId")??String(charts.find(c=>c.name==="Operations ledger")?.id??charts[0]?.id??"");
    const [invoices, payments, report] = await Promise.all([
      financeInvoiceListNative(actor),
      listPayments(actor),
      statements(actor,undefined,chartId),
    ]);
    return Response.json(canonicalDestinationLinks({ data: { charts, journals: native.journals, invoices, payments, report } }));
  } catch (error) {
    return errorResponse(error);
  }
}
