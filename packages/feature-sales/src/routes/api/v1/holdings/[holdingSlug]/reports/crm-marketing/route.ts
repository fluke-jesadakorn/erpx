import { canonicalDestinationLinks } from '@erpx/feature-work_projection/lib/destination-links';
import { requireWorkContext } from '@erpx/server-identity_access/public/authorization';
import { errorResponse } from '@erpx/server-runtime/public/http';
import { oasisDemoStagingEnabled } from '@erpx/contracts/holding-hosts';
import { crmAnalytics, marketingAnalytics } from '@erpx/server-crm/public/crm-marketing-report';
import { crmReportCompatibilityQuery, legacyCrmReportFilters, nativeCrmReport } from '../../../../../../../public/native-crm-report';

type Context = { params: Promise<{ holdingSlug: string }> };

export async function GET(request: Request, context: Context) {
  try {
    const actor = await requireWorkContext((await context.params).holdingSlug);
    const { kind, filters } = crmReportCompatibilityQuery(new URL(request.url).searchParams);
    const data = kind === 'marketing'
      ? await marketingAnalytics(actor, legacyCrmReportFilters(filters))
      : oasisDemoStagingEnabled(process.env)
        ? (await nativeCrmReport(actor, filters, request.headers)).data
        : await crmAnalytics(actor, legacyCrmReportFilters(filters));
    return Response.json(canonicalDestinationLinks({ data }), { headers: { 'Cache-Control': 'private, no-store' } });
  } catch (error) {
    return errorResponse(error);
  }
}
