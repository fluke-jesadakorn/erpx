import { canonicalDestinationLinks } from '@erpx/work-projection-ui/lib/destination-links';
import { requireWorkContext } from '@erpx/identity-access-server/public/authorization';
import { errorResponse } from '@erpx/platform-runtime-server/public/http';
import { oasisDemoStagingEnabled } from '@erpx/platform-contracts/holding-hosts';
import { crmAnalytics, marketingAnalytics } from '@erpx/crm-server/public/crm-marketing-report';
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
