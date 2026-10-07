import {z} from "zod";
import type {WorkContext} from "../../identity_access/public/authorization";
import type {ApprovalInstance,ApprovalStep} from "../../workflow/public/approvals";
import type {Clock,IdentifierSource} from "../../shared/application/ports";
import type {DocumentVersion,LegalItem} from "../domain/model";
import type {LegalWrite,LegalRows} from "./service";
export type LegalApprovalRepository={
routeSteps(facts:{instanceId:string;}):Promise<LegalRows<Record<string, unknown>>>;
decisionHistory(facts:{id:string;companyId:string;}):Promise<LegalRows<Record<string, unknown>>>;
contractDocument(facts:{documentId:string;itemId:string;}):Promise<DocumentVersion | null>;
executedDocument(facts:{id:string;}):Promise<Record<string, unknown> | null>;
latestAuthor(facts:{itemId:string;}):Promise<{ actor_id: string; } | null>;
signerAccess(facts:{itemId:string;u:string;itemId2:string;id:string;u2:string;now:number;}):Promise<Record<string, unknown> | null>;
lockDocument(facts:{id:string;}):LegalWrite;
retainRoute(facts:{itemId:string;instanceId:string;id:string;checksum:string;signerIdsJson:string;nextVersion:number;actorId:string;sourceHash:string;}):LegalWrite;
currentStep(facts:{instanceId:string;currentStep:number;}):Promise<ApprovalStep | null>;
routeSnapshot(facts:{itemId:string;instanceId:string;}):Promise<{ document_id: string; document_hash: string; signers: string; source_hash: string; } | null>;
documentAuthor(facts:{document_id:string;}):Promise<{ created_by: string; } | null>;
advanceRoute(facts:{now:number;instanceId:string;}):LegalWrite;
replaceReviewer(facts:{approverId:string;id:string;}):LegalWrite;
recordReroute(facts:{key:string;instanceId:string;id:string;actorId:string;reason:string;version:number;key2:string;now:number;approver_user_id:string;approverId:string;}):LegalWrite;
decideStep(facts:{value0:"approved" | "rejected";id:string;}):LegalWrite;
recordDecision(facts:{key:string;instanceId:string;id:string;decision:"approve" | "reject" | "revise";actorId:string;reason:string;version:number;key2:string;now:number;value9:string;}):LegalWrite;
nextReviewer(facts:{instanceId:string;nextStep:number;}):Promise<{ approver_user_id: string; } | null>;
cancelRoute(facts:{value0:"rejected" | "cancelled";now:number;instanceId:string;}):LegalWrite;
advanceStep(facts:{now:number;instanceId:string;}):LegalWrite;
notifyReviewer(facts:{identifier:string;companyId:string;approver_user_id:string;itemId:string;nextVersion:number;now:number;}):LegalWrite;
approveRoute(facts:{now:number;instanceId:string;}):LegalWrite;
recordApproval(facts:{itemId:string;nextVersion:number;document_id:string;document_hash:string;signerIdsJson:string;actorId:string;now:number;}):LegalWrite;
advanceContract(facts:{state:string;now:number;itemId:string;}):LegalWrite;
reviewerMembership(facts:{company_id:string;itemId:string;id:string;now:number;now2:number;}):Promise<Record<string, unknown> | null>;
sameAuthor(facts:{user:string;author:string;company:string;}):Promise<Record<string, unknown> | null>;
requireReviewerMembership(key:string,reviewerId:string,itemId:string):LegalWrite;
requireCurrentRoute(key:string,instanceId:string,currentStep:number,version:number):LegalWrite;
requireReplacementMembership(key:string,itemId:string,reviewerId:string):LegalWrite;
requireDecisionMembership(key:string,itemId:string):LegalWrite;
currentApproval(id:string,companyId:string):Promise<ApprovalInstance|null>;
openApproval(key:string,recordType:"legal",id:string,policyKey:"request",requesterId:string,subjectId:string|null,route:{scope:{company_id:string;branch_id:string|null;department_id:string|null};steps:{userId:string;authority:"named";snapshot:{documentId:string;checksum:string;signers:string[]}}[];blockedReason:null},version:number):Promise<{instanceId:string;statements:LegalWrite[]}>;
guardChanges(id:string,version:number,key:string,edit?:boolean):LegalWrite[];
personGuard(key:string,userId:string,scope:LegalItem,permission:string):LegalWrite;
event(id:string,action:string,note:string,version:number,identifier:string,now:number):LegalWrite;
replay(key:string,operation:string,id:string,input:unknown):Promise<Record<string,unknown>|null>;
commit<Result extends object>(key:string,operation:string,input:unknown,id:string,version:number,result:Result,writes:LegalWrite[]):Promise<Result>;
};
export function createLegalApprovalApplication(ports:{actor:WorkContext;repository:LegalApprovalRepository;clock:Clock;identifiers:IdentifierSource;hash(value:string):string;samePerson(userId:string|null|undefined):boolean;access(id:string,permission?:string):Promise<LegalItem>;eligible(id:string,item:LegalItem,permission:string):Promise<void>;error(status:number,code:string,message:string):Error}){const a=ports.actor;
function fail(message:string,code="LEGAL_INVALID",status=422):never{throw ports.error(status,code,message);}
function approvalDigest(
  item: LegalItem,
  documentId: string,
  checksum: string,
  signers: string[],
) {
  const { version: _version, updated_at: _updated, ...source } = item;
  void _version;
  void _updated;
  return ports.hash(JSON.stringify({ source, documentId, checksum, signers }));
}
async function legalApprovalView(
  id: string,
) {
  await ports.access(id);
  const instance = await ports.repository.currentApproval(id,a.companyId);
  return {
    instance,
    steps: instance
      ? (
          await ports.repository.routeSteps({instanceId:instance.id})
        ).results
      : [],
    history: (
      await ports.repository.decisionHistory({id:id,companyId:a.companyId})
    ).results,
  };
}
async function legalApprovalCommand(
  raw: unknown,
  key: string,
) {
  const x = z
    .object({
      action: z.enum(["approval.open", "approval.decide", "approval.reroute"]),
      id: z.string(),
      expectedVersion: z.number().int().positive(),
      reason: z.string().trim().min(3).max(4000),
    })
    .parse(raw);
  const item = await ports.access(x.id,x.action === "approval.open" ? "legal.review" : "legal.approve");
  const replay = await ports.repository.replay(key,x.action,item.id,raw);
  if (replay) return replay;
  if (item.kind !== "contract") fail("Contract approval requires a contract.");
  const steps: LegalWrite[] = ports.repository.guardChanges(item.id,x.expectedVersion,key,false);
  let state = item.state;
  if (x.action === "approval.open") {
    if (!["draft", "review", "approval"].includes(item.state))
      fail("Return this contract to draft before opening a new approval.");
    const p = z
      .object({
        documentId: z.string(),
        signerIds: z.array(z.string()).min(1).max(20),
        approverIds: z.array(z.string()).min(1).max(10),
      })
      .parse(raw);
    if (
      new Set(p.signerIds).size !== p.signerIds.length ||
      new Set(p.approverIds).size !== p.approverIds.length
    )
      fail("Signers and approvers must be unique.");
    const d = await ports.repository.contractDocument({documentId:p.documentId,itemId:item.id});
    if (!d) fail("Select a version on this contract.");
    if(await ports.repository.executedDocument({id:d.id}))fail("Imported executed copies are reviewed separately from draft approval.");
    const author = await ports.repository.latestAuthor({itemId:item.id});
    for (const u of p.approverIds) {
      await reviewer( item, u);
      steps.push(ports.repository.personGuard(key,u,item,"legal.approve"),ports.repository.requireReviewerMembership(key,u,item.id));
      if (
        await isAuthor( a.companyId, u, [
          a.userId,
          item.created_by,
          d.created_by,
          author?.actor_id,
        ])
      )
        fail(
          "Content authors cannot approve their own work.",
          "LEGAL_INDEPENDENT_REVIEW",
          403,
        );
    }
    for (const u of p.signerIds)
      if (
        !(await ports.repository.signerAccess({itemId:item.id,u:u,itemId2:item.id,id:d.id,u2:u,now:ports.clock.now()}))
      )
        fail("Each signer must have explicit access to this version.");
    const opened = await ports.repository.openApproval(key,"legal",item.id,"request",a.userId,null,{
        scope: {
          company_id: item.company_id,
          branch_id: item.branch_id,
          department_id: item.department_id,
        },
        steps: p.approverIds.map((userId) => ({
          userId,
          authority: "named" as const,
          snapshot: {
            documentId: d.id,
            checksum: d.checksum,
            signers: p.signerIds,
          },
        })),
        blockedReason: null,
      },item.version + 1);
    const after = { ...item, state: "approval" };
    steps.push(
      ...opened.statements,
      ports.repository.lockDocument({id:d.id}),
      ports.repository.retainRoute({itemId:item.id,instanceId:opened.instanceId,id:d.id,checksum:d.checksum,signerIdsJson:JSON.stringify(p.signerIds),nextVersion:item.version + 1,actorId:a.userId,sourceHash:approvalDigest(after, d.id, d.checksum, p.signerIds)}),
    );
    state = "approval";
  } else {
    const instance = await ports.repository.currentApproval(item.id,a.companyId);
    if (!instance || instance.state !== "pending" || item.state !== "approval")
      fail("No active approval route.");
    const current = await ports.repository.currentStep({instanceId:instance.id,currentStep:instance.current_step});
    if (!current) fail("Approval step unavailable.");
    const route = await ports.repository.routeSnapshot({itemId:item.id,instanceId:instance.id});
    if (!route) fail("Approval snapshot unavailable.");
    const signers =
      typeof route.signers === "string"
        ? (JSON.parse(route.signers) as string[])
        : route.signers;
    if (
      approvalDigest(item, route.document_id, route.document_hash, signers) !==
      route.source_hash
    )
      fail(
        "Material contract data changed. Open a fresh approval.",
        "LEGAL_APPROVAL_STALE",
        409,
      );
    steps.push(
      ports.repository.requireCurrentRoute(key,instance.id,instance.current_step,instance.version),
    );
    if (x.action === "approval.reroute") {
      const p = z.object({ approverId: z.string() }).parse(raw);
      await reviewer( item, p.approverId);
      steps.push(ports.repository.personGuard(key,p.approverId,item,"legal.approve"),ports.repository.requireReplacementMembership(key,item.id,p.approverId));
      const author = await ports.repository.documentAuthor({document_id:route.document_id});
      if (
        await isAuthor( a.companyId, p.approverId, [
          item.created_by,
          instance.requester_user_id,
          author?.created_by,
        ])
      )
        fail("The replacement must be independent.");
      steps.push(
        ports.repository.advanceRoute({now:ports.clock.now(),instanceId:instance.id}),
        ports.repository.replaceReviewer({approverId:p.approverId,id:current.id}),
        ports.repository.recordReroute({key:key,instanceId:instance.id,id:current.id,actorId:a.userId,reason:x.reason,version:item.version,key2:key,now:ports.clock.now(),approver_user_id:current.approver_user_id,approverId:p.approverId}),
      );
    } else {
      const decision = z
        .enum(["approve", "reject", "revise"])
        .parse((raw as { decision: unknown }).decision);
      if (!ports.samePerson(current.approver_user_id))
        fail(
          "This decision is assigned to another reviewer.",
          "LEGAL_APPROVER",
          403,
        );
      await reviewer( item, a.userId);
      steps.push(ports.repository.personGuard(key,a.userId,item,"legal.approve"),ports.repository.requireDecisionMembership(key,item.id));
      if (
        ports.samePerson(item.created_by) ||
        ports.samePerson(instance.requester_user_id)
      )
        fail(
          "An independent reviewer is required.",
          "LEGAL_INDEPENDENT_REVIEW",
          403,
        );
      steps.push(
        ports.repository.decideStep({value0:decision === "approve" ? "approved" : "rejected",id:current.id}),
        ports.repository.recordDecision({key:key,instanceId:instance.id,id:current.id,decision:decision,actorId:a.userId,reason:x.reason,version:item.version,key2:key,now:ports.clock.now(),value9:JSON.stringify({
              role: a.role,
              assignmentId: a.assignmentId,
              documentId: route.document_id,
              documentHash: route.document_hash,
            })}),
      );
      const next = await ports.repository.nextReviewer({instanceId:instance.id,nextStep:instance.current_step + 1});
      if (decision !== "approve") {
        state = decision === "revise" ? "draft" : "rejected";
        steps.push(
          ports.repository.cancelRoute({value0:decision === "revise" ? "cancelled" : "rejected",now:ports.clock.now(),instanceId:instance.id}),
        );
      } else if (next) {
        steps.push(
          ports.repository.advanceStep({now:ports.clock.now(),instanceId:instance.id}),
          ports.repository.notifyReviewer({identifier:ports.identifiers.next(),companyId:a.companyId,approver_user_id:next.approver_user_id,itemId:item.id,nextVersion:item.version + 1,now:ports.clock.now()}),
        );
      } else {
        state = "approved";
        steps.push(
          ports.repository.approveRoute({now:ports.clock.now(),instanceId:instance.id}),
          ports.repository.recordApproval({itemId:item.id,nextVersion:item.version + 1,document_id:route.document_id,document_hash:route.document_hash,signerIdsJson:JSON.stringify(signers),actorId:a.userId,now:ports.clock.now()}),
        );
      }
    }
  }
  steps.push(
    ports.repository.advanceContract({state:state,now:ports.clock.now(),itemId:item.id}),
    ports.repository.event(item.id,x.action,x.reason,item.version + 1,ports.identifiers.next(),ports.clock.now()),
  );
  return ports.repository.commit(key,x.action,raw,item.id,item.version + 1,{ id: item.id, version: item.version + 1, state },steps);
}
async function reviewer(
  item: LegalItem,
  id: string,
) {
  await ports.eligible(id,item,"legal.approve");
  if (
    !(await ports.repository.reviewerMembership({company_id:item.company_id,itemId:item.id,id:id,now:ports.clock.now(),now2:ports.clock.now()}))
  )
    fail(
      "Reviewer needs explicit record membership and an active Legal approval role.",
    );
}

async function isAuthor(
  company: string,
  user: string,
  authors: (string | undefined)[],
) {
  for (const author of authors)
    if (
      author &&
      (await ports.repository.sameAuthor({user:user,author:author,company:company}))
    )
      return true;
  return false;
}

return {approvalDigest,legalApprovalView,legalApprovalCommand};}
