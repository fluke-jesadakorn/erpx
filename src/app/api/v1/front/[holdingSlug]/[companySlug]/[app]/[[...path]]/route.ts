import { z } from "zod";
import { ensureFrontRole, frontCompanyBrand, frontLocationScope, frontOperationAllowed, frontRoleState, type FrontAppKind } from "@/server/front-app";
import { requireWorkContext } from "@/server/authorization";
import { requestPublicOrigin } from "@/server/public-origin";
import { ApiError, errorResponse } from "@/server/http";
import { holdingResources } from "@/server/runtime";
import { PostgresDatabase } from "@/server/runtime/postgres";
import { posWorkspace } from "@/server/pos";
import { posCommand } from "@/server/pos-core";
import {staffBookingChange} from "@/server/pms/staff-booking-changes";
import { pmsWorkspace, pmsCommand } from "@/server/pms";
import { pmsReadSelectionFromSearch } from "@/lib/pms-read-selection";
import { staffBookingSearch, bookingError } from "@/server/pms-booking";
import { isTransactionalSqlDatabase } from "@/server/runtime/types";
import { selectWorkRole } from "@/server/work-role-store";
import { frontActionItems } from "@/server/front-quick-access";
import { readQuickAccessPreferences, saveQuickAccessPreferences } from "@/server/quick-access-preferences";
import { pmsHomeRead, pmsHomeWrite } from "@/server/pms/home-api";
import { posCustomers } from '@/server/pos-daily';
import { posProductImage } from '@/server/pos-images';
import {frontTaxDocumentCommand,frontTaxDocumentView} from '@/server/hospitality-tax-documents';
import {taxDocumentRequest} from '@/server/hospitality-tax-document-request';
import {frontPmsAttachmentPaths,frontPmsAttachmentRequest} from '@/server/pms/front-attachments';

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
    if(app==='pms'&&frontPmsAttachmentPaths.has(path[0]))return await frontPmsAttachmentRequest(request,await frontActor(request,holdingSlug,companySlug,app),path);
    if(path.length===1&&path[0]==='tax-documents'){
      const actor=await frontActor(request,holdingSlug,companySlug,app);
      const invoiceId=z.string().uuid().parse(new URL(request.url).searchParams.get('invoiceId'));
      return Response.json({data:await frontTaxDocumentView(actor,invoiceId,app)},{headers:{'Cache-Control':'private, no-store'}});
    }
    if (app === "pms" && path[0] === "home") {
      const actor = await frontActor(request, holdingSlug, companySlug, app);
      return Response.json({data:await pmsHomeRead(actor,path.slice(1),new URL(request.url).searchParams)}, {headers:{"Cache-Control":"private, no-store"}});
    }
    if (path.length === 1 && path[0] === "roles") {
      const brand = await frontCompanyBrand(holdingSlug, companySlug);
      if (!brand) throw new ApiError(404, "FRONT_NOT_FOUND", "This Front app is unavailable.");
      const location = request.headers.get("x-erpx-front-location")?.trim() || "holding";
      const state = await ensureFrontRole(holdingSlug, brand.companyId, app, location);
      return Response.json({ data: {
        valid: !!state.selected,
        revision: state.selection?.revision ?? 0,
        assignmentId: state.selection?.assignment_id ?? null,
        companyId: brand.companyId,
      } }, { headers: { "Cache-Control": "private, no-store" } });
    }
    if (path.length === 1 && path[0] === "quick-access") {
      const actor = await frontActor(request, holdingSlug, companySlug, app);
      return Response.json({ data: await readQuickAccessPreferences(actor.userId, "front-" + app, actor.companyId) }, { headers: { "Cache-Control": "private, no-store" } });
    }
    if(app==='pos'&&path[0]==='images'&&path.length===2)return posProductImage(await frontActor(request,holdingSlug,companySlug,app),path[1]);
    if(app==='pos'&&path.join('/')==='customers/search'){
      const actor=await frontActor(request,holdingSlug,companySlug,app),db=(await holdingResources(actor.holdingId)).write;
      return Response.json({data:await posCustomers(db,actor,new URL(request.url).searchParams.get('q')??'')},{headers:{'Cache-Control':'private, no-store'}});
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
        { data: await posWorkspace(actor, search.get("sale") ?? undefined, search.get("q") ?? "", tab, search.get("cursor") ?? undefined, true) },
        { headers: { "Cache-Control": "private, no-store" } },
      );
    }
    return Response.json(
      { data: await pmsWorkspace(actor, search.get("property") ?? undefined, search.get("dayControls") === "1", true, pmsReadSelectionFromSearch(search)) },
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
      return Response.json({data:await pmsHomeWrite(actor,path.slice(1),await request.json(),commandKey(request))}, {headers:{"Cache-Control":"private, no-store"}});
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
      return Response.json({ data: { assignmentId: assignment.id, revision: selected.revision } }, { headers: { "Cache-Control": "private, no-store" } });
    }
    const actor = await frontActor(request, holdingSlug, companySlug, app);

    if(path.length===1&&path[0]==='tax-documents'){
      const {command,key}=await taxDocumentRequest(request);
      return Response.json({data:await frontTaxDocumentCommand(actor,command,key,app)},{headers:{'Cache-Control':'private, no-store'}});
    }

    if (path.length === 1 && path[0] === "quick-access") {
      const input = z.object({
        pinnedIds: z.array(z.string().min(1).max(128)).max(6).nullable(),
        expectedVersion: z.number().int().nonnegative(),
      }).parse(await request.json());
      const availableIds = frontActionItems(app, actor, "").map((item) => item.id);
      return Response.json({ data: await saveQuickAccessPreferences({
        userId: actor.userId,
        surface: "front-" + app,
        contextKey: actor.companyId,
        pinnedIds: input.pinnedIds,
        expectedVersion: input.expectedVersion,
        availableIds,
        requestKey: commandKey(request),
      }) }, { headers: { "Cache-Control": "private, no-store" } });
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
        { data: await posCommand(db, actor, operation, body, commandKey(request)) },
        { headers: { "Cache-Control": "private, no-store" } },
      );
    }

    if(path.length===1&&path[0]==='booking-changes'){
      const db=(await holdingResources(actor.holdingId)).write;
      if(!isTransactionalSqlDatabase(db))throw new Error('POSTGRES_REQUIRED');
      return Response.json({data:await staffBookingChange(db,actor,holdingSlug,await request.json(),commandKey(request))},{headers:{'Cache-Control':'private, no-store'}});
    }

    if (path.length === 1 && (path[0] === "availability" || path[0] === "quote")) {
      const value = z.object({ propertyId: z.string().min(1), input: z.unknown() }).parse(await request.json());
      return Response.json(
        { data: await staffBookingSearch(actor, value.propertyId, path[0], value.input) },
        { headers: { "Cache-Control": "private, no-store" } },
      );
    }

    if (path.length)
      throw new ApiError(403, "FRONT_COMMAND_FORBIDDEN", "Configuration and approvals are handled in ERPX.");
    const key = commandKey(request);
    const body = await request.json();
    const operation = typeof body?.operation === "string" ? body.operation : "";
    if (!frontOperationAllowed(app, operation))
      throw new ApiError(403, "FRONT_COMMAND_FORBIDDEN", "Configuration and approvals are handled in ERPX.");
    const db = (await holdingResources(actor.holdingId)).write;
    if (!isTransactionalSqlDatabase(db)) throw new Error("POSTGRES_REQUIRED");
    return Response.json(
      { data: await pmsCommand(db, actor, body, key) },
      { headers: { "Cache-Control": "private, no-store" } },
    );
  } catch (error) {
    return errorResponse(error instanceof z.ZodError ? new ApiError(422, "VALIDATION_FAILED", error.issues[0]?.message ?? "Invalid input.") : bookingError(error));
  }
}
