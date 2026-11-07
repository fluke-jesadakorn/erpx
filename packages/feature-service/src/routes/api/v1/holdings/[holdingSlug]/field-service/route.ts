import { nativeReservePost, nativeReserveReadback, nativeIssueReadback, nativeRequestReadback, nativeOrderCreateReadback, nativeVisitReadback, nativeBillingReadback, nativeAgreementReviewReadback } from "./native-reserve";
import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import { requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { requestPublicOrigin } from "@erpx/server-manage_erpx/public/public-origin";
import { ApiError, errorResponse } from "@erpx/server-runtime/public/http";
import {nativeServiceWorkspace,nativeServiceDetail,NativeServiceWorkspaceError} from '@erpx/feature-field_service/public/native-workspace';
import {partsSetupContext} from '../../../../../../lib/parts-setup-context';

type C = { params: Promise<{ holdingSlug: string }> };
const headers = { "Cache-Control": "private, no-store" };
export async function GET(r: Request, c: C) {
  try {
    const slug=(await c.params).holdingSlug, query=new URL(r.url).searchParams;
    const billingKey=query.get("nativeBillingKey");
    if(billingKey!==null) {
      if(!billingKey||[...query.keys()].length!==1) throw new ApiError(404,"NOT_FOUND","Service order not found.");
      const native=await nativeBillingReadback(r,slug,billingKey);
      if(native)return native;
      throw new ApiError(404,"NOT_FOUND","Service order not found.");
    }
    const agreementCreateKey=query.get("nativeAgreementCreateKey");
    if(agreementCreateKey!==null) {
      if(!agreementCreateKey||[...query.keys()].length!==1) throw new ApiError(404,"NOT_FOUND","Service agreement not found.");
      const native=await nativeAgreementReviewReadback(r,slug,agreementCreateKey,true);
      if(native)return native;
      throw new ApiError(404,"NOT_FOUND","Service agreement not found.");
    }
    const agreementReviewKey=query.get("nativeAgreementReviewKey");
    if(agreementReviewKey!==null) {
      if(!agreementReviewKey||[...query.keys()].length!==1) throw new ApiError(404,"NOT_FOUND","Service agreement not found.");
      const native=await nativeAgreementReviewReadback(r,slug,agreementReviewKey);
      if(native)return native;
      throw new ApiError(404,"NOT_FOUND","Service agreement not found.");
    }
    const cancelKey=query.get("nativeCancelKey");
    if(cancelKey!==null) {
      if(!cancelKey||[...query.keys()].length!==1) throw new ApiError(404,"NOT_FOUND","Service order not found.");
      const native=await nativeVisitReadback(r,slug,cancelKey,'cancel');
      if(native)return native;
      throw new ApiError(404,"NOT_FOUND","Service order not found.");
    }
    const recoveryKey=query.get("nativeVisitRecoveryKey");
    if(recoveryKey!==null) {
      if(!recoveryKey||[...query.keys()].length!==1) throw new ApiError(404,"NOT_FOUND","Service order not found.");
      const native=await nativeVisitReadback(r,slug,recoveryKey,'recovery');
      if(native)return native;
      throw new ApiError(404,"NOT_FOUND","Service order not found.");
    }
    const completionKey=query.get("nativeVisitCompletionKey");
    if(completionKey!==null) {
      if(!completionKey||[...query.keys()].length!==1) throw new ApiError(404,"NOT_FOUND","Service order not found.");
      const native=await nativeVisitReadback(r,slug,completionKey,'completion');
      if(native)return native;
      throw new ApiError(404,"NOT_FOUND","Service order not found.");
    }
    const visitKey=query.get("nativeVisitKey");
    if(visitKey!==null) {
      if(!visitKey||[...query.keys()].length!==1) throw new ApiError(404,"NOT_FOUND","Service order not found.");
      const native=await nativeVisitReadback(r,slug,visitKey);
      if(native)return native;
      throw new ApiError(404,"NOT_FOUND","Service order not found.");
    }
    const orderCreateKey=query.get("nativeOrderCreateKey");
    if(orderCreateKey!==null) {
      if(!orderCreateKey||[...query.keys()].length!==1) throw new ApiError(404,"NOT_FOUND","Service order not found.");
      const native=await nativeOrderCreateReadback(r,slug,orderCreateKey);
      if(native)return native;
      throw new ApiError(404,"NOT_FOUND","Service order not found.");
    }
    const requestKey=query.get("nativeRequestKey");
    if(requestKey!==null) {
      if(!requestKey||[...query.keys()].length!==1) throw new ApiError(404,"NOT_FOUND","Service request not found.");
      const native=await nativeRequestReadback(r,slug,requestKey);
      if(native)return native;
      throw new ApiError(404,"NOT_FOUND","Service request not found.");
    }
    const issueOrder=query.get("nativeIssueOrder"), issueKey=query.get("nativeIssueKey");
    if(issueOrder!==null || issueKey!==null) {
      if(!issueOrder||!issueKey||[...query.keys()].length!==2) throw new ApiError(404,"NOT_FOUND","Service order not found.");
      const native=await nativeIssueReadback(r,slug,issueOrder,issueKey);
      if(native)return native;
      throw new ApiError(404,"NOT_FOUND","Service order not found.");
    }
    const order=query.get("nativeReserveOrder"), reservation=query.get("nativeReserveId");
    if(order!==null || reservation!==null) {
      if(!order||!reservation||[...query.keys()].length!==2) throw new ApiError(404,"NOT_FOUND","Service order not found.");
      const native=await nativeReserveReadback(r,slug,order,reservation);
      if(native)return native;
      throw new ApiError(404,"NOT_FOUND","Service order not found.");
    }
    const actor=await requireWorkContext(slug),id=query.get("id"),context=partsSetupContext(actor);
    const data=id?await nativeServiceDetail(slug,id,context):await nativeServiceWorkspace(slug,context);
    if(data===undefined)throw new ApiError(503,"NATIVE_SERVICE_UNAVAILABLE","The native Service workspace is unavailable.");
    return Response.json(canonicalDestinationLinks({data}),{headers});
  } catch (e) {
    return errorResponse(e instanceof NativeServiceWorkspaceError?new ApiError(e.status??503,e.code,"The native Service workspace is unavailable."):e);
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
    const holdingSlug=(await c.params).holdingSlug;
    await requireWorkContext(holdingSlug);
    const original=r.clone(),input=await r.json();
    const native=await nativeReservePost(original,holdingSlug,input);
    if(native)return native;
    void original.body?.cancel().catch(()=>undefined);
    throw new ApiError(503,"NATIVE_SERVICE_UNAVAILABLE","This Service command requires its configured native capability.");
  } catch (e) {
    return errorResponse(e instanceof NativeServiceWorkspaceError?new ApiError(e.status??503,e.code,"The native Service workspace is unavailable."):e);
  }
}
