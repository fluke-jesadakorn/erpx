import {billingInput,type BillingActor,type BillingInput} from '../domain/billing';
import type {BillingView} from "@/lib/billing/billing";
import {billingMonth} from "@/lib/billing/billing";
import type {WorkContext} from '@/server/modules/identity_access/public/authorization';
import type {BillingEvidenceSource} from '../domain/evidence';
import type {TargetBillingPeriodView} from '../domain/target';
export interface BillingApiRepository {platformAccounts():Promise<{rows:Record<string,unknown>[]}>;holdingSlug(id:string):Promise<{rows:{slug:string}[]}>;scopedEvidence(id:string,holdingId:string):Promise<{rows:BillingEvidenceSource[]}>;deniedUnavailable(id:string,evidenceId:string,holdingId:string,actorId:string,reason:string,now:number):Promise<unknown>;deniedMissingFile(id:string,evidenceId:string,holdingId:string,actorId:string,reason:string,now:number):Promise<unknown>;allowedEvidence(id:string,evidenceId:string,holdingId:string,actorId:string,now:number):Promise<unknown>;existingEvidence(id:string):Promise<{rows:BillingEvidenceSource[]}>;billingAccountExists(holdingId:string):Promise<{rowCount:number|null}>;retainQuarantine(facts:{id:string;holdingId:string;filename:string;mime:string;size:number;sha:string;actorId:string;now:number;objectKey:string;detected:string}):Promise<void>;}
export interface BillingApiPorts {ready():Promise<void>;repository:BillingApiRepository;platformActor():Promise<{userId:string;email:string;role:string}>;workActor(slug:string):Promise<WorkContext>;companyAuthority(actor:WorkContext):Promise<unknown>;error(status:number,code:string,message:string):Error;errorResponse(error:unknown):Response;now():number;id():string;origin():string;publicOrigin(request:Request):string;readOnly():boolean;uploadsFrozen():boolean;scannerReadiness():Promise<{ready:boolean;enabled:boolean}>;object(key:string):Promise<{body:ReadableStream<Uint8Array>}|null>;objectHead(key:string):Promise<{customMetadata:Record<string,string>}|null>;putObject(key:string,bytes:Uint8Array,options:{onlyIf:{etagDoesNotMatch:string};httpMetadata:{contentType:string;cacheControl:string};customMetadata:{sha256:string;state:string}}):Promise<unknown>;hash(value:string|Uint8Array):string;detect(bytes:Uint8Array,mime:string):string|null;target(holdingId:string,month:string):Promise<TargetBillingPeriodView>;view(holdingId:string):Promise<BillingView>;body(request:Request):Promise<unknown>;command(actor:BillingActor,holdingId:string,input:BillingInput,key:string,returnUrl:string):Promise<{ok:boolean;url?:string}>;}
function concatBytes(chunks:Uint8Array[],size:number){const bytes=new Uint8Array(size);let offset=0;for(const chunk of chunks){bytes.set(chunk,offset);offset+=chunk.byteLength;}return bytes;}
export function createBillingApi(ports:BillingApiPorts){
function billingHandler(platform: boolean) {
  return async (
    request: Request,
    context: { params: Promise<{ holdingSlug?: string; path?: string[] }> },
  ) => {
    try {
      const params = await context.params,
        
        url = new URL(request.url),
        path = params.path ?? [];
      await ports.ready();
      let holdingId: string | undefined,
        slug = params.holdingSlug,
        actor: BillingActor;
      if (platform) {
        const a = await ports.platformActor();
        if (
          !["platform_admin", "platform_auditor"].includes(a.role) ||
          (request.method !== "GET" && a.role !== "platform_admin")
        )
          throw ports.error(
            403,
            "FORBIDDEN",
            "Platform billing permission required.",
          );
        actor = { userId: a.userId, email: a.email, platform: true };
        holdingId = url.searchParams.get("holdingId") ?? undefined;
      } else {
        const a = await ports.workActor(slug!);
        if (a.role !== "holding_owner")
          throw ports.error(
            403,
            "FORBIDDEN",
            "Only the Holding owner can access billing.",
          );
        await ports.companyAuthority(a);
        holdingId = a.holdingId;
        actor = {
          userId: a.userId,
          email: a.email,
          platform: false,
          holdingId,
        };
      }
      if (platform && !holdingId && request.method === "GET" && !path.length)
        return Response.json({
          data: {
            accounts: (
              await ports.repository.platformAccounts()
            ).rows,
          },
        });
      if (platform && holdingId === "h003")
        throw ports.error(403, "ERPX_NOT_CUSTOMER", "ERPX is not a customer billing account.");
      if (!holdingId)
        throw ports.error(422, "HOLDING_REQUIRED", "Select a Holding.");
      const holding = (
        await ports.repository.holdingSlug(holdingId)
      ).rows[0];
      if (!holding) throw ports.error(404, "NOT_FOUND", "Holding not found.");
      slug = holding.slug;
      if(request.method==='GET'&&path.length===1&&path[0]==='target'){
        const month=url.searchParams.get('month')??billingMonth(ports.now());
        if(!/^\d{4}-(0[1-9]|1[0-2])$/.test(month))throw ports.error(422,'INVALID_BILLING_MONTH','Choose a valid billing month.');
        return Response.json({data:await ports.target(holdingId,month)},{headers:{'Cache-Control':'private, no-store'}});
      }
      if (path[0] === "evidence") {
        if (!platform)
          throw ports.error(
            403,
            "FORBIDDEN",
            "Evidence is private to Platform billing.",
          );
        
        if (request.method === "GET" && path[1]) {
          const row = (
            await ports.repository.scopedEvidence(path[1],holdingId)
          ).rows[0];
          if (!row) throw ports.error(404, "NOT_FOUND", "Evidence not found.");
          if(row.source_state!=="available"){
            await ports.repository.deniedUnavailable(`billing-access-${ports.id()}`,row.id,holdingId,actor.userId,"EVIDENCE_NOT_AVAILABLE",ports.now());
            throw ports.error(423,"EVIDENCE_NOT_AVAILABLE","Evidence is quarantined, rejected, or awaiting a clean scan.");
          }
          const file = await ports.object(row.object_key??`${holdingId}/${row.id}`);
          if (!file){
            await ports.repository.deniedMissingFile(`billing-access-${ports.id()}`,row.id,holdingId,actor.userId,"FILE_UNAVAILABLE",ports.now());
            throw ports.error(404, "NOT_FOUND", "Evidence file missing.");
          }
          await ports.repository.allowedEvidence(`billing-access-${ports.id()}`,row.id,holdingId,actor.userId,ports.now());
          return new Response(file.body, {
            headers: {
              "Content-Type": row.mime,
              "Content-Disposition": `attachment; filename*=UTF-8''${encodeURIComponent(row.filename)}`,
              "Cache-Control": "private, no-store",
              "X-Content-Type-Options": "nosniff",
            },
          });
        }
        if (request.method === "POST" && path.length === 1) {
          if(ports.readOnly())throw ports.error(503,"ATTACHMENTS_READ_ONLY","Billing evidence intake is disabled during attachment rollback. Existing clean evidence remains readable.");
          if(ports.uploadsFrozen())throw ports.error(503,"ATTACHMENT_UPLOAD_FREEZE","Billing evidence intake is briefly paused for final storage reconciliation.");
          if (
            request.headers.get("origin") &&
            request.headers.get("origin") !== ports.publicOrigin(request)
          )
            throw ports.error(
              403,
              "ORIGIN_FORBIDDEN",
              "Use the application origin.",
            );
          const key = request.headers.get("Idempotency-Key") ?? "";
          if (!/^[a-zA-Z0-9:_.-]{8,180}$/.test(key))
            throw ports.error(
              422,
              "KEY_REQUIRED",
              "Upload command key required.",
            );
          const mime = request.headers.get("content-type") ?? "",
            filename = decodeURIComponent(
              request.headers.get("x-filename") ?? "evidence",
            );
          if (
            !["application/pdf", "image/png", "image/jpeg"].includes(mime) ||
            filename.length > 200
          )
            throw ports.error(
              422,
              "INVALID_FILE",
              "Use a PDF, PNG or JPEG up to 10 MiB.",
            );
          const chunks: Uint8Array[] = [];
          let size = 0;
          const reader = request.body?.getReader();
          if (reader)
            while (true) {
              const r = await reader.read();
              if (r.done) break;
              size += r.value.length;
              if (size > 10 * 1024 * 1024) {
                await reader.cancel();
                throw ports.error(413, "FILE_TOO_LARGE", "Maximum 10 MiB.");
              }
              chunks.push(r.value);
            }
          const bytes = concatBytes(chunks,size);
          const detected=ports.detect(bytes,mime);
          if (detected!==mime)
            throw ports.error(
              422,
              "INVALID_FILE",
              "The file contents do not match its type.",
            );
          const id = ports.hash(`${holdingId}:${actor.userId}:${key}`),
            sha = ports.hash(bytes);
          const old = (
            await ports.repository.existingEvidence(id)
          ).rows[0];
          if (
            old &&
            (old.sha256 !== sha ||
              old.filename !== filename ||
              old.mime !== mime)
          )
            throw ports.error(409, "KEY_CONFLICT", "Upload key already used.");
          if(old){
            const state=String(old.source_state),pending=["legacy_unscanned","quarantined","scanning","scan_error"].includes(state);
            return Response.json({data:{id:old.id,state}},{status:pending?202:200});
          }
          if (
            !(
              await ports.repository.billingAccountExists(holdingId)
            ).rowCount
          )
            throw ports.error(
              409,
              "ACCOUNT_REQUIRED",
              "Enroll this Holding before adding evidence.",
            );
          const readiness=await ports.scannerReadiness();
          if(!readiness.ready||!readiness.enabled)throw ports.error(503,"ATTACHMENT_SCANNER_UNAVAILABLE","Billing evidence intake requires a healthy malware scanner and current signatures.");
          const objectKey=`${holdingId}/quarantine/${id}/original`;
          const saved = await ports.putObject(objectKey, bytes, {
            onlyIf: { etagDoesNotMatch: "*" },
            httpMetadata: { contentType: mime,cacheControl:"private, no-store" },customMetadata:{sha256:sha,state:"quarantined"},
          });
          if (!saved) {
            const existing = await ports.objectHead(objectKey);
            if (existing?.customMetadata.sha256 !== sha)
              throw ports.error(
                409,
                "KEY_CONFLICT",
                "Upload key already used.",
              );
          }
          const now=ports.now();await ports.repository.retainQuarantine({id,holdingId,filename,mime,size,sha,actorId:actor.userId,now,objectKey,detected});
          return Response.json({ data: { id,state:"quarantined" } },{status:202});
        }
      }
      if (path.length)
        throw ports.error(404, "NOT_FOUND", "Billing route not found.");
      if (request.method === "GET")
        return Response.json(
          { data: await ports.view(holdingId) },
          { headers: { "Cache-Control": "private, no-store" } },
        );
      const input = billingInput.parse(await ports.body(request));
      const data = await ports.command(
        actor,
        holdingId,
        input,
        request.headers.get("Idempotency-Key") ?? "",
        new URL(`/holdings/${slug}/settings?section=usage`, ports.origin())
          .href,
      );
      return Response.json({ data });
    } catch (error) {
      return ports.errorResponse(error);
    }
  };
}

return {billingHandler};}
