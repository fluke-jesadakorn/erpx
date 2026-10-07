import {z} from "zod";
import type {WorkContext} from "../../identity_access/public/authorization";
import type {Clock,IdentifierSource} from "../../shared/application/ports";
import type {LegalWrite,LegalRows} from "./service";
import {dateSchema,type LegalItem} from "../domain/model";
export type LegalGovernanceRepository={
forms(facts:{companyId:string;}):Promise<LegalRows<Record<string, unknown>>>;
authorities(facts:{id:string;}):Promise<LegalRows<Record<string, unknown>>>;
settings(facts:{companyId:string;}):Promise<Record<string, unknown> | null>;
createForm(facts:{key:string;companyId:string;name:string;fieldsJson:string;slaDays:number;actorId:string;}):LegalWrite;
form(facts:{formId:string;companyId:string;}):Promise<{ created_by: string; state: string; version: number; } | null>;
approveForm(facts:{actorId:string;formId:string;}):LegalWrite;
approvedForm(facts:{formId:string;companyId:string;}):Promise<{ fields: string; sla_days: number; } | null>;
escalationMember(facts:{itemId:string;escalationOwner:string;}):Promise<Record<string, unknown> | null>;
applyIntake(facts:{formId:string;answersJson:string;now:number;fact3:string;fact4:string | null;itemId:string;}):LegalWrite;
authorityDocument(facts:{documentId:string;itemId:string;}):Promise<Record<string, unknown> | null>;
signerAccess(facts:{itemId:string;userId:string;itemId2:string;userId2:string;now:number;}):Promise<Record<string, unknown> | null>;
createAuthority(facts:{key:string;itemId:string;companyId:string;userId:string;documentId:string;currency:string;limitMinor:number;validFrom:string;validUntil:string;actorId:string;}):LegalWrite;
authority(facts:{authorityId:string;itemId:string;}):Promise<{ created_by: string; state: string; version: number; } | null>;
decideAuthority(facts:{fact0:"approved" | "revoked";fact1:string | null;authorityId:string;}):LegalWrite;
advanceRecord(facts:{now:number;itemId:string;}):LegalWrite;
requireFormCurrent(key:string,values:(string | number)[]):LegalWrite;
requireAuthorityCurrent(key:string,values:(string | number)[]):LegalWrite;
guardChanges(id:string,version:number,key:string,edit?:boolean):LegalWrite[];
companyLock():LegalWrite;
personGuard(key:string,userId:string,scope?:LegalItem,permission?:string):LegalWrite;
event(id:string,action:string,note:string,version:number,identifier:string,now:number):LegalWrite;
replay<Result extends object=Record<string,unknown>>(key:string,operation:string,id:string,input:unknown):Promise<Result|null>;
commit<Result extends object>(key:string,operation:string,input:unknown,id:string,version:number,result:Result,writes:LegalWrite[]):Promise<Result>;
};
export function createLegalGovernanceApplication(ports:{actor:WorkContext;repository:LegalGovernanceRepository;clock:Clock;identifiers:IdentifierSource;samePerson(userId:string|null|undefined):boolean;require(permission:string):void;access(id:string,permission?:string):Promise<LegalItem>;eligible(id:string,item?:LegalItem,permission?:string):Promise<void>;error(status:number,code:string,message:string):Error}){const a=ports.actor;
function fail(message:string,code="LEGAL_INVALID",status=422):never{throw ports.error(status,code,message);}
async function governanceView(
  id?: string,
) {
  ports.require("legal.read");
  if (id) await ports.access(id);
  return {
    forms: (
      await ports.repository.forms({companyId:a.companyId})
    ).results.map((r) => ({
      ...r,
      fields: typeof r.fields === "string" ? JSON.parse(r.fields) : r.fields,
    })),
    authorities: id
      ? (
          await ports.repository.authorities({id:id})
        ).results
      : [],
    settings: await ports.repository.settings({companyId:a.companyId}),
  };
}
async function governanceCommand(
  raw: unknown,
  key: string,
) {
  const x = z
    .object({
      action: z.enum([
        "form.create",
        "form.approve",
        "intake.apply",
        "authority.create",
        "authority.approve",
        "authority.revoke",
      ]),
      id: z.string().optional(),
      expectedVersion: z.number().int().positive().optional(),
      reason: z.string().trim().min(3).max(4000),
    })
    .parse(raw);
  const formAction = x.action.startsWith("form.");
  ports.require(formAction
      ? "legal.manage"
      : x.action === "authority.approve"
        ? "legal.approve"
        : "legal.edit");
  const item = formAction ? null : await ports.access(z.string().parse(x.id));
  const aggregate = item?.id ?? a.companyId;
  const prior = await ports.repository.replay(key,x.action,aggregate,raw);
  if (prior) return prior;
  const steps = item
    ? ports.repository.guardChanges(item.id,z.number().parse(x.expectedVersion),key,x.action !== "authority.approve")
    : [ports.repository.companyLock()];
  if (formAction && (a.branchId || a.departmentId))
    fail("Manage intake forms at Company scope.");
  if (x.action === "form.create") {
    const p = z
      .object({
        name: z.string().min(3).max(200),
        slaDays: z.coerce.number().int().min(1).max(365),
        fields: z
          .array(
            z.object({
              key: z.string().regex(/^[a-zA-Z][a-zA-Z0-9_]{0,39}$/),
              label: z.string().min(1).max(200),
              required: z.boolean(),
              type: z.enum(["text", "date", "number"]),
            }),
          )
          .min(1)
          .max(30),
      })
      .parse(raw);
    if (new Set(p.fields.map((f) => f.key)).size !== p.fields.length)
      fail("Field keys must be unique.");
    steps.push(
      ports.repository.createForm({key:key,companyId:a.companyId,name:p.name,fieldsJson:JSON.stringify(p.fields),slaDays:p.slaDays,actorId:a.userId}),
    );
  } else if (x.action === "form.approve") {
    const p = z
        .object({
          formId: z.string(),
          formVersion: z.number().int().positive(),
        })
        .parse(raw),
      f = await ports.repository.form({formId:p.formId,companyId:a.companyId});
    if (!f || f.state !== "draft" || f.version !== p.formVersion)
      fail("Form changed.");
    if (ports.samePerson(f.created_by))
      fail("An independent manager must approve this form.");
    steps.push(
      ports.repository.requireFormCurrent(key,[p.formId, p.formVersion]),
      ports.repository.approveForm({actorId:a.userId,formId:p.formId}),
    );
  } else if (x.action === "intake.apply") {
    if (
      item!.kind !== "request" ||
      !["draft", "submitted", "triage"].includes(item!.state)
    )
      fail("Apply an intake form while the request is being prepared.");
    const p = z
        .object({
          formId: z.string(),
          answers: z.record(z.string(), z.string().max(12000)),
          escalationOwner: z.string().optional(),
        })
        .parse(raw),
      f = await ports.repository.approvedForm({formId:p.formId,companyId:a.companyId});
    if (!f) fail("Choose an approved form.");
    const fields = JSON.parse(f.fields) as {
      key: string;
      required: boolean;
      type: string;
    }[];
    if (Object.keys(p.answers).some((k) => !fields.some((f) => f.key === k)))
      fail("Unknown intake field.");
    for (const field of fields) {
      const answer = p.answers[field.key] ?? "";
      if (field.required && !answer.trim()) fail(`Complete ${field.key}.`);
      if (answer && field.type === "date") dateSchema.parse(answer);
      if (answer && field.type === "number" && !/^\d+(\.\d+)?$/.test(answer))
        fail(`Invalid number for ${field.key}.`);
    }
    if (
      p.escalationOwner &&
      !(await ports.repository.escalationMember({itemId:item!.id,escalationOwner:p.escalationOwner}))
    )
      fail("Escalation owner needs explicit access.");
    steps.push(
      ports.repository.applyIntake({formId:p.formId,answersJson:JSON.stringify(p.answers),now:ports.clock.now(),fact3:String(f.sla_days),fact4:p.escalationOwner ?? null,itemId:item!.id}),
    );
  } else if (x.action === "authority.create") {
    if (!["contract", "corporate"].includes(item!.kind))
      fail("Record authority on a contract or corporate record.");
    const p = z
      .object({
        userId: z.string(),
        documentId: z.string(),
        currency: z.string().regex(/^[A-Z]{3}$/),
        limitMinor: z.coerce.number().int().min(0).max(Number.MAX_SAFE_INTEGER),
        validFrom: dateSchema,
        validUntil: dateSchema,
      })
      .parse(raw);
    if (p.validUntil < p.validFrom) fail("Authority validity is reversed.");
    if (
      !(await ports.repository.authorityDocument({documentId:p.documentId,itemId:item!.id}))
    )
      fail("Attach the authority evidence to this record first.");
    if (
      !(await ports.repository.signerAccess({itemId:item!.id,userId:p.userId,itemId2:item!.id,userId2:p.userId,now:ports.clock.now()}))
    )
      fail("The signer needs explicit access.");
    steps.push(
      ports.repository.createAuthority({key:key,itemId:item!.id,companyId:a.companyId,userId:p.userId,documentId:p.documentId,currency:p.currency,limitMinor:p.limitMinor,validFrom:p.validFrom,validUntil:p.validUntil,actorId:a.userId}),
    );
  } else {
    const p = z
        .object({
          authorityId: z.string(),
          authorityVersion: z.number().int().positive(),
        })
        .parse(raw),
      r = await ports.repository.authority({authorityId:p.authorityId,itemId:item!.id});
    if (!r || r.version !== p.authorityVersion) fail("Authority changed.");
    if (
      x.action === "authority.approve" &&
      (r.state !== "proposed" || ports.samePerson(r.created_by))
    )
      fail("An independent reviewer must approve the proposed authority.");
    steps.push(
      ports.repository.requireAuthorityCurrent(key,[p.authorityId, p.authorityVersion]),
      ports.repository.decideAuthority({fact0:x.action === "authority.approve" ? "approved" : "revoked",fact1:x.action === "authority.approve" ? a.userId : null,authorityId:p.authorityId}),
    );
  }
  if (item)
    steps.push(
      ports.repository.advanceRecord({now:ports.clock.now(),itemId:item.id}),
      ports.repository.event(item.id,x.action,x.reason,item.version + 1,ports.identifiers.next(),ports.clock.now()),
    );
  return ports.repository.commit(key,x.action,raw,aggregate,item ? item.version + 1 : 1,{ id: aggregate, version: item ? item.version + 1 : 1 },steps);
}

return {governanceView,governanceCommand};}
