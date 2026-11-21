import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import { requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { requestPublicOrigin } from "@erpx/server-manage_erpx/public/public-origin";
import { ApiError, errorResponse } from "@erpx/server-runtime/public/http";
import {
  changeMarketing,
  createMarketingAsset,
  createMarketingCandidate,
  marketingDetail,
  marketingWorkspace,
} from "@erpx/server-marketing/public/marketing";
import {changeMarketingCampaign} from "@erpx/server-marketing/public/marketing-campaigns";
import {
  nativeMarketingLeadAssetCreate,
  nativeMarketingLeadAssetReview,
  nativeMarketingLeadCampaignCreate,
  nativeMarketingLeadCandidateCreate,
  nativeMarketingLeadFailure,
  nativeMarketingLeadRecordEngagement,
} from "@erpx/feature-marketing/public/native-marketing-lead";
type C = { params: Promise<{ holdingSlug: string }> };
const headers = { "Cache-Control": "private, no-store" };
const assetKinds = ["form", "content", "domain", "provider"] as const;
const assetActions = ["activate", "reject", "retire"] as const;
const object = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === "object" && !Array.isArray(value);
const strings = (value: unknown): Record<string, string> =>
  Object.fromEntries(
    Object.entries(object(value) ? value : {}).map(([name, item]) => [name, String(item)]),
  );
export async function GET(r: Request, c: C) {
  try {
    const a = await requireWorkContext((await c.params).holdingSlug),
      id = new URL(r.url).searchParams.get("id");
    return Response.json(
      canonicalDestinationLinks({ data: id ? await marketingDetail(a, id) : await marketingWorkspace(a) }),
      { headers },
    );
  } catch (e) {
    return errorResponse(e);
  }
}
export async function POST(r: Request, c: C) {
  try {
    if (
      r.headers.get("origin") &&
      r.headers.get("origin") !== requestPublicOrigin(r)
    )
      throw new ApiError(
        403,
        "ORIGIN_FORBIDDEN",
        "Use the application origin.",
      );
    const key = r.headers.get("Idempotency-Key");
    if (!key)
      throw new ApiError(
        422,
        "COMMAND_KEY_REQUIRED",
        "Idempotency-Key is required.",
      );
    const a = await requireWorkContext((await c.params).holdingSlug),
      raw = await r.json(),
      i: Record<string, unknown> = object(raw) ? raw : {};
    // Native fence: campaign/candidate/asset creation, the asset review
    // transition and the candidate engagement command are owner-qualified.
    // Campaign cost linking, candidate handoff, unknown asset actions and the
    // public form stay on the retained TypeScript owner.
    const attempt = async (run: () => Promise<unknown>) => {
      try {
        const data = await run();
        if (data)
          return Response.json(canonicalDestinationLinks({ data }), {
            headers,
          });
      } catch (e) {
        const failure = nativeMarketingLeadFailure(e);
        if (failure)
          throw new ApiError(failure.status, failure.code, failure.message);
        throw e;
      }
      return undefined;
    };
    if (
      i.entity === "candidate" &&
      i.action === "record-engagement" &&
      typeof i.id === "string"
    ) {
      const response = await attempt(() =>
        nativeMarketingLeadRecordEngagement(
          r.headers,
          a,
          i.id as string,
          {
            expectedVersion: Number(i.expectedVersion),
            eventType: String(i.eventType ?? ""),
            channel: String(i.channel ?? ""),
            reason: String(i.reason ?? ""),
          },
          key,
        ),
      );
      if (response) return response;
    }
    if (i.entity === "campaign" && i.action === "create") {
      const response = await attempt(() =>
        nativeMarketingLeadCampaignCreate(
          r.headers,
          a,
          { code: String(i.code ?? ""), name: String(i.name ?? "") },
          key,
        ),
      );
      if (response) return response;
    }
    if (
      i.entity === "asset" &&
      typeof i.id === "string" &&
      assetActions.some((action) => action === i.action)
    ) {
      const response = await attempt(() =>
        nativeMarketingLeadAssetReview(
          r.headers,
          a,
          i.id as string,
          {
            action: String(i.action) as (typeof assetActions)[number],
            expectedVersion: Number(i.expectedVersion),
            reason: String(i.reason ?? ""),
          },
          key,
        ),
      );
      if (response) return response;
    }
    if (
      i.entity === "asset" &&
      typeof i.id !== "string" &&
      (i.action === undefined || i.action === "create")
    ) {
      if (!object(i.configuration) || !object(i.preview))
        throw new ApiError(
          422,
          "MARKETING_ASSET_INPUT_INVALID",
          "Review the Marketing asset profile.",
        );
      const configuration = i.configuration,
        preview = i.preview,
        kind = assetKinds.find((value) => value === i.kind) ?? String(i.kind ?? "");
      const response = await attempt(() =>
        nativeMarketingLeadAssetCreate(
          r.headers,
          a,
          {
            kind: kind as (typeof assetKinds)[number],
            name: String(i.name ?? ""),
            configuration,
            preview,
          },
          key,
        ),
      );
      if (response) return response;
    }
    if (i.entity === "candidate" && typeof i.id !== "string") {
      const response = await attempt(() =>
        nativeMarketingLeadCandidateCreate(
          r.headers,
          a,
          {
            formId: String(i.formId ?? ""),
            formVersion: Number(i.formVersion),
            branchId: String(i.branchId ?? ""),
            contactName: String(i.contactName ?? ""),
            contactDetail: String(i.contactDetail ?? ""),
            consentEvidence: String(i.consentEvidence ?? ""),
            attribution: strings(i.attribution),
            brief: String(i.brief ?? ""),
          },
          key,
        ),
      );
      if (response) return response;
    }
    const data =
      i.entity === 'campaign'
        ? await changeMarketingCampaign(a,i,key)
        : typeof i.id === "string"
        ? await changeMarketing(a, i.entity as "asset" | "candidate", i.id, i, key)
        : i.entity === "asset"
          ? await createMarketingAsset(a, i, key)
          : await createMarketingCandidate(a, i, key);
    return Response.json(canonicalDestinationLinks({ data }), { headers });
  } catch (e) {
    return errorResponse(e);
  }
}
