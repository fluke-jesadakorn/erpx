import {canonicalDestinationLinks} from "@/lib/work_projection/destination-links";
import { roomChargeReversalQueue } from "@/server/modules/pos/public/room-charge-reversal-queue";
import { canonicalGuestAccount, guestAccountOperation } from "@/lib/identity_access/guest-account-compat";
import { z } from "zod";
import { ensureFrontRole, frontCompanyBrand, frontLocationScope, frontOperationAllowed, frontRoleState, type FrontAppKind } from "@/server/modules/work_projection/public/front-app";
import { requireWorkContext } from "@/server/modules/identity_access/public/authorization";
import { requestPublicOrigin } from "@/server/modules/manage_erpx/public/public-origin";
import { ApiError, errorResponse } from "@/server/modules/runtime/public/http";
import { holdingResources } from "@/server/modules/runtime/public/runtime";
import { PostgresDatabase } from "@/server/modules/runtime/public/runtime/postgres";
import { posWorkspace } from "@/server/modules/pos/public/pos";
import { posCommand } from "@/server/modules/pos/public/pos-core";
import {staffBookingChange} from "@/server/modules/pms/public/pms/staff-booking-changes";
import { pmsWorkspace, pmsCommand } from "@/server/modules/pms/public/pms";
import { pmsReadSelectionFromSearch } from "@/server/modules/pms/public/rules/pms-read-selection";
import { staffBookingSearch, bookingError } from "@/server/modules/pms/public/pms-booking";
import { isTransactionalSqlDatabase } from "@/server/modules/runtime/public/runtime/types";
import { selectWorkRole } from "@/server/modules/work_projection/public/work-role-store";
import {frontCompanyQuickAccessRequest} from '@/server/modules/work_projection/public/front-quick-access-request';
import {boundedBody} from '@/server/modules/legal/public/legal/body';
import { pmsHomeRead, pmsHomeWrite } from "@/server/modules/pms/public/pms/home-api";
import { posCustomers } from "@/server/modules/pos/public/pos-daily";
import { posProductImage } from "@/server/modules/pos/public/pos-images";
import {frontTaxDocumentCommand,frontTaxDocumentView} from "@/server/modules/finance/public/hospitality-tax-documents";
import {taxDocumentRequest} from "@/server/modules/finance/public/hospitality-tax-document-request";
import {frontPmsAttachmentPaths,frontPmsAttachmentRequest} from "@/server/modules/pms/public/pms/front-attachments";
import { moneyContext, preferenceSchema, saveCurrencyPreference } from "@/server/modules/finance/public/company-money";

export const dynamic = "force-dynamic";

type Context = {
  params: Promise<{
    holdingSlug: string;
    companySlug: string;
    app: string;
    path?: string[];
  }>;
};

function assertOrigin(request: Request) {
  const origin = request.headers.get("origin");
  if (origin !== requestPublicOrigin(request))
    throw new ApiError(403, "ORIGIN_FORBIDDEN", "Use the application origin.");
}

async function frontActor(
  request: Request,
  holdingSlug: string,
  companySlug: string,
  app: FrontAppKind,
) {
  const brand = await frontCompanyBrand(holdingSlug, companySlug);
  if (!brand)
    throw new ApiError(404, "FRONT_NOT_FOUND", "This Front app is unavailable.");
  const requestedCompany = request.headers.get("x-erpx-front-company");
  if (requestedCompany && requestedCompany !== companySlug)
    throw new ApiError(404, "FRONT_NOT_FOUND", "This Front app is unavailable.");
  const contentLocation = app==='pms'&&/\/attachment-links\/[^/]+\/content$/.test(new URL(request.url).pathname)
    ? new URL(request.url).searchParams.get('frontLocation') : null;
  const location = request.headers.get("x-erpx-front-location")?.trim() || contentLocation || "holding";
  const scope = frontLocationScope(holdingSlug, location);
  if (!scope)
    throw new ApiError(404, "FRONT_LOCATION_NOT_FOUND", "This Front location is unavailable.");
  await ensureFrontRole(holdingSlug, brand.companyId, app, location);
  const actor = await requireWorkContext(scope);
  if (actor.companyId !== brand.companyId)
    throw new ApiError(404, "FRONT_NOT_FOUND", "This Front app is unavailable.");
  return actor;
}

function commandKey(request: Request) {
  const key = request.headers.get("Idempotency-Key");
  if (!key)
    throw new ApiError(422, "IDEMPOTENCY_KEY_REQUIRED", "An Idempotency-Key is required.");
  return key;
}

export async function PUT(request:Request,{params}:Context){
 try{
  assertOrigin(request);
  const {holdingSlug,companySlug,app,path=[]}=await params;
  if(app!=='pms'||path[0]!=='uploads')throw new ApiError(404,'NOT_FOUND','Hotel upload not found.');
  return await frontPmsAttachmentRequest(request,await frontActor(request,holdingSlug,companySlug,'pms'),path);
 }catch(error){return errorResponse(error);}
}

export async function GET(request: Request, { params }: Context) {
  try {
    const { holdingSlug, companySlug, app: rawApp, path = [] } = await params;
    if (rawApp !== "pos" && rawApp !== "pms")
      throw new ApiError(404, "NOT_FOUND", "Not found.");
    const app = rawApp as FrontAppKind;
    if (path.join("/") === "room-charge-reversals") {
      const actor=await frontActor(request,holdingSlug,companySlug,app);
      const data=await roomChargeReversalQueue(actor,new URL(request.url).searchParams.get("property")??undefined);
      return Response.json(canonicalDestinationLinks({data:data.filter(row=>row.stage===app)}),{headers:{"Cache-Control":"private, no-store"}});
    }
    if (path.length === 1 && path[0] === "money") {
      const actor = await frontActor(request, holdingSlug, companySlug, app);
      return Response.json(canonicalDestinationLinks({ data: { ...await moneyContext(actor), canConfirmRate: false } }), { headers: { "Cache-Control": "private, no-store" } });
    }
    if(app==='pms'&&frontPmsAttachmentPaths.has(path[0]))return await frontPmsAttachmentRequest(request,await frontActor(request,holdingSlug,companySlug,app),path);
    if(path.length===1&&path[0]==='tax-documents'){
      const actor=await frontActor(request,holdingSlug,companySlug,app);
      const invoiceId=z.string().uuid().parse(new URL(request.url).searchParams.get('invoiceId'));
      return Response.json(canonicalDestinationLinks({data:await frontTaxDocumentView(actor,invoiceId,app)}),{headers:{'Cache-Control':'private, no-store'}});
    }
    if (app === "pms" && path[0] === "home") {
      const actor = await frontActor(request, holdingSlug, companySlug, app);
      return Response.json(canonicalDestinationLinks({data:await pmsHomeRead(actor,path.slice(1),new URL(request.url).searchParams)}), {headers:{"Cache-Control":"private, no-store"}});
    }
    if (path.length === 1 && path[0] === "roles") {
      const brand = await frontCompanyBrand(holdingSlug, companySlug);
      if (!brand) throw new ApiError(404, "FRONT_NOT_FOUND", "This Front app is unavailable.");
      const location = request.headers.get("x-erpx-front-location")?.trim() || "holding";
      const state = await ensureFrontRole(holdingSlug, brand.companyId, app, location);
      return Response.json(canonicalDestinationLinks({ data: {
        valid: !!state.selected,
        userId: state.access.userId,
        revision: state.selection?.revision ?? 0,
        assignmentId: state.selection?.assignment_id ?? null,
        companyId: brand.companyId,
      } }), { headers: { "Cache-Control": "private, no-store" } });
    }
    if (path.length === 1 && path[0] === "quick-access") {
      const actor = await frontActor(request, holdingSlug, companySlug, app);
      return Response.json(canonicalDestinationLinks({ data: await frontCompanyQuickAccessRequest(request,app,actor,()=>frontActor(request,holdingSlug,companySlug,app)) }), { headers: { "Cache-Control": "private, no-store" } });
    }
    if(app==='pos'&&path[0]==='images'&&path.length===2)return posProductImage(await frontActor(request,holdingSlug,companySlug,app),path[1]);
    if(app==='pos'&&path.join('/')==='customers/search'){
      const actor=await frontActor(request,holdingSlug,companySlug,app),db=(await holdingResources(actor.holdingId)).write;
      return Response.json(canonicalDestinationLinks({data:await posCustomers(db,actor,new URL(request.url).searchParams.get('q')??'')}),{headers:{'Cache-Control':'private, no-store'}});
    }
    if (path.length)
      throw new ApiError(403, "FRONT_ROUTE_FORBIDDEN", "This work belongs in ERPX administration.");
    const actor = await frontActor(request, holdingSlug, companySlug, app);

    const search = new URL(request.url).searchParams;
    if (app === "pos") {
      const tab = search.get("tab") ?? "sale";
      if (!["sale", "kitchen", "shifts", "history"].includes(tab))
        throw new ApiError(403, "FRONT_ROUTE_FORBIDDEN", "This view belongs in ERPX administration.");
      return Response.json(
        canonicalDestinationLinks({ data: await posWorkspace(actor, search.get("sale") ?? undefined, search.get("q") ?? "", tab, search.get("cursor") ?? undefined, true) }),
        { headers: { "Cache-Control": "private, no-store" } },
      );
    }
    return Response.json(
      canonicalDestinationLinks({ data: await pmsWorkspace(actor, search.get("property") ?? undefined, search.get("dayControls") === "1", true, pmsReadSelectionFromSearch(search)) }),
      { headers: { "Cache-Control": "private, no-store" } },
    );
  } catch (error) {
    return errorResponse(error);
  }
}

export async function POST(request: Request, { params }: Context) {
  try {
    assertOrigin(request);
    const { holdingSlug, companySlug, app: rawApp, path = [] } = await params;
    if (rawApp !== "pos" && rawApp !== "pms")
      throw new ApiError(404, "NOT_FOUND", "Not found.");
    const app = rawApp as FrontAppKind;
    if(app==='pms'&&frontPmsAttachmentPaths.has(path[0]))return await frontPmsAttachmentRequest(request,await frontActor(request,holdingSlug,companySlug,app),path);
    if (app === "pms" && path[0] === "home") {
      const actor = await frontActor(request, holdingSlug, companySlug, app);
      const version=path.length===4&&path[1]==='handover'&&path[3]==='acknowledgement'?request.headers.get('x-erpx-handover-version'):null;
      if(version!==null&&(!/^[1-9][0-9]{0,15}$/.test(version)||!Number.isSafeInteger(Number(version))))throw new ApiError(422,'HANDOVER_VERSION_INVALID','Provide the handover version shown on this page.');
      return Response.json(canonicalDestinationLinks({data:await pmsHomeWrite(actor,path.slice(1),await request.json(),commandKey(request),version===null?undefined:Number(version))}), {headers:{"Cache-Control":"private, no-store"}});
    }
    if (path.length === 1 && path[0] === "roles") {
      const brand = await frontCompanyBrand(holdingSlug, companySlug);
      if (!brand) throw new ApiError(404, "FRONT_NOT_FOUND", "This Front app is unavailable.");
      const input = z.object({ location: z.string().min(1).max(256), assignmentId: z.string().min(1).max(128), revision: z.number().int().nonnegative() }).parse(await request.json());
      const state = await frontRoleState(holdingSlug, brand.companyId, app, input.location);
      const assignment = state.options.find((item) => item.id === input.assignmentId);
      if (!assignment) throw new ApiError(403, "ASSIGNMENT_UNAVAILABLE", "Choose an active Front role for this location.");
      const selected = await selectWorkRole(state.write, {
        sessionId: state.access.sessionId, userId: state.access.userId, companyId: brand.companyId,
        assignmentId: assignment.id, assignmentVersion: assignment.version!, revision: input.revision,
        expiresAt: state.access.expiresAt, rememberPreference: false,
      });
      return Response.json(canonicalDestinationLinks({ data: { assignmentId: assignment.id, revision: selected.revision } }), { headers: { "Cache-Control": "private, no-store" } });
    }
    const actor = await frontActor(request, holdingSlug, companySlug, app);

    if (path[0] === "money") {
      if (path.length !== 2 || path[1] !== "preference")
        throw new ApiError(403, "FRONT_COMMAND_FORBIDDEN", "Currency administration is handled in ERPX.");
      const input = await request.json().catch(() => { throw new ApiError(422, "INVALID_JSON", "Use a JSON body."); });
      const parsed = preferenceSchema.safeParse(input);
      if (!parsed.success) throw new ApiError(422, "INVALID_CURRENCY", parsed.error.issues[0]?.message ?? "Invalid currency.");
      return Response.json(canonicalDestinationLinks({ data: await saveCurrencyPreference(actor, parsed.data, commandKey(request)) }), { headers: { "Cache-Control": "private, no-store" } });
    }

    if(path.length===1&&path[0]==='tax-documents'){
      const {command,key}=await taxDocumentRequest(request);
      return Response.json(canonicalDestinationLinks({data:await frontTaxDocumentCommand(actor,command,key,app)}),{headers:{'Cache-Control':'private, no-store'}});
    }

    if (path.length === 1 && path[0] === "quick-access") {
      const preference=await frontCompanyQuickAccessRequest(request,app,actor,()=>frontActor(request,holdingSlug,companySlug,app),async()=>{
        let raw:unknown;try{raw=JSON.parse(new TextDecoder().decode(await boundedBody(request,8192)));}catch(error){if(error instanceof SyntaxError)throw new ApiError(422,'INVALID_JSON','Use a JSON body.');throw error;}
        const input = z.object({
          pinnedIds: z.array(z.string().min(1).max(128)).max(6).nullable(),
          expectedVersion: z.number().int().nonnegative(),
        }).parse(raw);
        return {...input,requestKey:commandKey(request)};
      });
      return Response.json(canonicalDestinationLinks({ data: preference }), { headers: { "Cache-Control": "private, no-store" } });
    }

    if (app === "pms" && path.length===2 && path[0]==="room_charge_reversal" && ["approve_pms","reject_pms"].includes(path[1])) {
      const db=(await holdingResources(actor.holdingId)).write;
      if(!isTransactionalSqlDatabase(db))throw new Error("POSTGRES_REQUIRED");
      return Response.json(canonicalDestinationLinks({data:await posCommand(db as PostgresDatabase,actor,path.join("."),await request.json(),commandKey(request))}),{headers:{"Cache-Control":"private, no-store"}});
    }
    if (app === "pos") {
      if (path.length !== 2)
        throw new ApiError(403, "FRONT_COMMAND_FORBIDDEN", "Configuration and approvals are handled in ERPX.");
      const operation = path.join(".");
      if (!frontOperationAllowed(app, operation))
        throw new ApiError(403, "FRONT_COMMAND_FORBIDDEN", "Configuration and approvals are handled in ERPX.");
      const body = await request.json();
      const db = (await holdingResources(actor.holdingId)).write as PostgresDatabase;
      return Response.json(
        canonicalDestinationLinks({ data: await posCommand(db, actor, operation, body, commandKey(request)) }),
        { headers: { "Cache-Control": "private, no-store" } },
      );
    }

    if(path.length===1&&path[0]==='booking-changes'){
      const db=(await holdingResources(actor.holdingId)).write;
      if(!isTransactionalSqlDatabase(db))throw new Error('POSTGRES_REQUIRED');
      return Response.json(canonicalDestinationLinks({data:await staffBookingChange(db,actor,holdingSlug,await request.json(),commandKey(request))}),{headers:{'Cache-Control':'private, no-store'}});
    }

    if (path.length === 1 && (path[0] === "availability" || path[0] === "quote")) {
      const value = z.object({ propertyId: z.string().min(1), input: z.unknown() }).parse(await request.json());
      return Response.json(
        canonicalDestinationLinks({ data: await staffBookingSearch(actor, value.propertyId, path[0], value.input) }),
        { headers: { "Cache-Control": "private, no-store" } },
      );
    }

    if (path.length)
      throw new ApiError(403, "FRONT_COMMAND_FORBIDDEN", "Configuration and approvals are handled in ERPX.");
    const key = commandKey(request);
    const body = canonicalGuestAccount(await request.json());
    const operation = typeof body?.operation === "string" ? guestAccountOperation(body.operation) : "";
    if (!frontOperationAllowed(app, operation))
      throw new ApiError(403, "FRONT_COMMAND_FORBIDDEN", "Configuration and approvals are handled in ERPX.");
    const db = (await holdingResources(actor.holdingId)).write;
    if (!isTransactionalSqlDatabase(db)) throw new Error("POSTGRES_REQUIRED");
    return Response.json(
      canonicalDestinationLinks({ data: await pmsCommand(db, actor, body, key) }),
      { headers: { "Cache-Control": "private, no-store" } },
    );
  } catch (error) {
    return errorResponse(error instanceof z.ZodError ? new ApiError(422, "VALIDATION_FAILED", error.issues[0]?.message ?? "Invalid input.") : bookingError(error));
  }
}
