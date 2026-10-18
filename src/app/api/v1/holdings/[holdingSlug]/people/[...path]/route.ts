import {payrollPostingPrerequisiteConfirmation,payrollPostingPrerequisiteConfirmationSchema,payrollPostingPrerequisites,createPayrollPostingChart,createPayrollPostingAccount,createPayrollPostingPeriod,confirmPayrollPostingRate,payrollPostingPrerequisitesSelectionSchema,payrollPostingChartSchema,payrollPostingAccountSchema,payrollPostingPeriodSchema,payrollPostingRateSchema} from '@/server/modules/finance/public/payroll-posting-prerequisites';
import {getFinalPayCancellation,cancelFinalPayPreparation,finalPayCancellationSchema} from '@/server/modules/people_payroll/public/final-pay-cancellation';
import {getPayrollOtherLiabilities,remitPayrollOtherLiability,importPayrollOtherLiabilityBankEvidence,payrollOtherRemittanceSchema,payrollOtherBankEvidenceSchema,ignorePayrollOtherLiabilityBankEvidence,payrollOtherBankEvidenceIgnoreSchema} from '@/server/modules/people_payroll/public/payroll-other-remittance';
import {canonicalDestinationLinks} from "@/lib/work_projection/destination-links";
import { organizationDraftApi } from "@/server/modules/organization/public/organization-drafts";
import { rehireInput, createRehireEngagement, peopleLifecycleData, previewPackage, savePackage, bindPositionPackage, previewLifecycle, saveLifecycle, cancelLifecycle, requestLifecycleAccess, lifecycleSettingsInput, saveLifecycleSettings, completePreparationTask } from "@/server/modules/people_payroll/public/people-lifecycle";
import { packageInput, lifecycleInput } from "@/lib/people_payroll/people-lifecycle";
import { placementBatchInput, previewPlacementBatch, submitPlacementBatch } from "@/server/modules/people_payroll/public/people-placement-batch";
import { positionMoveInput, positionMoveSubmitInput } from "@/lib/people_payroll/position-moves";
import { previewPositionMove, submitPositionMove, positionMoves, cancelPositionMove } from "@/server/modules/organization/public/position-moves";
import type { PostgresDatabase } from "@/server/modules/runtime/public/runtime/postgres";
import { directoryOperationsApi } from "@/server/modules/identity_access/public/directory-operations-api";
import { peopleTemplatesData,saveHoldingTemplate,adoptHoldingTemplate,adoptTemplateInput } from "@/server/modules/people_payroll/public/people-templates";
import { iamActorFromWork } from "@/server/modules/identity_access/public/iam-context";
import { saveSpendingTemplate, spendingTemplateSchema } from "@/server/modules/people_payroll/public/spending-templates";
import { expenseRoute } from "@/server/modules/people_payroll/public/expense-routing";
import { requestPublicOrigin } from "@/server/modules/manage_erpx/public/public-origin";
import { holdingResources } from "@/server/modules/runtime/public/runtime";
import { organizationOverview, positionSchema, savePosition, organizationTemplateSchema, saveOrganizationTemplate, applyOrganizationTemplateSchema, applyOrganizationTemplate, installStaffLevels, organizationNodeChangeSchema, organizationNodeDecisionSchema, previewOrganizationNodeChange, saveOrganizationNodeChange, decideOrganizationNodeChange, cancelOrganizationNodeChange } from "@/server/modules/organization/public/organization";
import { companyBusinessAcceptance, companyAcceptanceSubmitSchema, companyAcceptanceDecisionSchema, companyAcceptanceRevokeSchema, submitCompanyBusinessAcceptance, decideCompanyBusinessAcceptance, revokeCompanyBusinessAcceptance } from "@/server/modules/finance/public/company-business-acceptance";
import { grantSchema,grantPersonAccess,revokePersonAccess,invitePerson,retryInvitationDelivery,revokeInvitation } from "@/server/modules/identity_access/public/account-access";
import { directorySchema, directoryUpdateSchema, directoryStatusSchema, saveDirectoryPerson, setDirectoryStatus } from "@/server/modules/people_payroll/public/people-directory";
import { z } from "zod";
import {getPayslip,getPayslipExport,listPayslips,issuePostedPayslips,issuePayslipSchema} from "@/server/modules/people_payroll/public/payroll-payslips";
import { cancelPayrollRun, payrollCancelSchema } from "@/server/modules/people_payroll/public/payroll-cancellation";
import { createPayrollAdjustment, editPayrollAdjustment, payrollAdjustmentCreateSchema, payrollAdjustmentEditSchema } from "@/server/modules/people_payroll/public/payroll-adjustments";
import {requestPayrollRecovery,consentPayrollRecovery,reviewPayrollRecovery,recognizePayrollRecovery,payrollRecoveryPage,payrollRecoveryAgreement,payrollRecoveryRequestSchema,payrollRecoveryConsentSchema,payrollRecoveryReviewSchema,payrollRecoveryRecognizeSchema} from "@/server/modules/people_payroll/public/payroll-recoveries";
import { thaiPayrollConfiguration,createThaiPayrollProfile,decideThaiPayrollProfile,thaiPayrollProfileSchema,thaiPayrollProfileDecisionSchema } from "@/server/modules/people_payroll/public/payroll-statutory";
import { consumeHireHandoff,hireConsumptionSchema,hireHandoffQueue } from "@/server/modules/people_payroll/public/people-handoffs";
import { hasPermission } from "@/lib/identity_access/permissions";
import { getPayrollSchedule, savePayrollSchedule, payrollScheduleSchema, payrollReadiness, submitPayrollReview, payrollSubmitReviewSchema } from "@/server/modules/people_payroll/public/payroll-readiness";
import { getPayrollLiabilities, remitPayrollLiability, payrollRemittanceSchema } from "@/server/modules/people_payroll/public/payroll-remittance";
import { positionPayInput, levelLeaveInput, savePositionPayDefault, saveLevelLeaveDefault } from "@/server/modules/organization/public/staff-defaults";
import { requireWorkContext } from "@/server/modules/identity_access/public/authorization";
import { requirePermission } from "@/server/modules/identity_access/public/iam-permissions";
import { ApiError, errorResponse } from "@/server/modules/runtime/public/http";
import {
  payrollOptions,assignPayGroupSchema,assignPayGroup,
  payrollCostAssignments,payrollCostVerificationSchema,verifyPayrollCostAssignment,
  compensationSchema,
  compensationTransitionSchema,
  confirmPaymentBatch,
  importSyntheticPaymentResults,
  countryPackSchema,
  createCompensation,
  createCountryPack,
  createEngagement,
  createPayrollRun,
  createPayGroup,
  createPerson,
  createPaymentBatch,
  preparePaymentRetry,
  createPersonSchema,
  createEngagementSchema,
  exportPaymentBatch,
  getPayrollRun,
  listPayrollRuns,
  listPeople,
  getPerson,
  peopleSummary,
  payrollSummary,
  payrollRunSchema,
  payGroupSchema,
  paymentBatchSchema,
  paymentConfirmationSchema,
  calculatePayrollRun,
  approvePayrollRun,
  prepareFinalPay,
  finalPayPreparationSchema,
  requestPayrollCorrection,
  decidePayrollCorrection,
  payrollCorrectionRequestSchema,
  payrollCorrectionDecisionSchema,
  exportPayrollReview,
  getPayrollReviewFile,
  payrollPostSchema,
  previewPayrollAccounting,
  payrollPostingOptions,
  payrollAccountMappingSchema,
  savePayrollAccountMapping,
  approvePayrollAccountMapping,
  postPayroll,
  approvePaymentBatch,
  getPaymentFile,
  listCountryPacks,
  transitionCountryPack,
  countryPackTransitionSchema,
  timeEntrySchema,
  timeEntryUpdateSchema, timeEntryReviewSchema, updateTimeEntry, rejectTimeEntry,
  createTimeEntry,
  approveTimeEntry,
  transitionCompensation,
  engagementTransitionSchema,
  transitionEngagement,
  requestPayrollAdvance,
  decidePayrollAdvance,
  payrollAdvanceRequestSchema,
  payrollAdvanceDecisionSchema,
  payrollAdvancePage,
} from "@/server/modules/people_payroll/public/people-payroll";

type Context = { params: Promise<{ holdingSlug: string; path: string[] }> };
function parts(path: string[]) { return path.filter(Boolean).map((part) => decodeURIComponent(part)); }
function command(request: Request) { const value = request.headers.get("Idempotency-Key"); if (!value) throw new ApiError(422,"IDEMPOTENCY_KEY_REQUIRED","Idempotency-Key is required."); return value; }
async function json(request: Request) { try { return await request.json(); } catch { throw new ApiError(422,"INVALID_JSON","Request body must be valid JSON."); } }
function parsed<T>(result: z.ZodSafeParseResult<T>): T {
  if (!result.success) throw new ApiError(422,"VALIDATION_FAILED",result.error.issues[0]?.message ?? "Invalid request.");
  return result.data;
}
const legacySupersedeInput = z.object({id:z.string().min(1),expectedVersion:z.number().int().positive()}).strict();

export const dynamic = "force-dynamic";

export async function GET(request: Request, context: Context) {
  try {
    const { holdingSlug, path } = await context.params; const actor = await requireWorkContext(holdingSlug); const p = parts(path);
    if(p[0]==='payroll'&&p[1]?.startsWith('posting-prerequisites')){if(p[1]!=='posting-prerequisites'||p.length!==2)throw new ApiError(404,'NOT_FOUND','People resource not found.');const query=new URL(request.url).searchParams;if([...query.keys()].length!==new Set(query.keys()).size)throw new ApiError(422,'VALIDATION_FAILED','Select one Payroll group.');return Response.json(canonicalDestinationLinks({data:await payrollPostingPrerequisites(actor,parsed(payrollPostingPrerequisitesSelectionSchema.safeParse(Object.fromEntries(query))))}),{headers:{'Cache-Control':'private, no-store'}});}
    if(p[0]==='payroll'&&p[1]==='runs'&&p[3]?.startsWith('final-pay-cancellation')){if(p[3]!=='final-pay-cancellation'||p.length!==4||!p[2])throw new ApiError(404,'NOT_FOUND','People resource not found.');return Response.json(canonicalDestinationLinks({data:await getFinalPayCancellation(actor,p[2])}),{headers:{'Cache-Control':'private, no-store'}});}
    if(p[0]==="hire-handoffs"&&p.length===1)return Response.json(canonicalDestinationLinks({data:await hireHandoffQueue(actor)}),{headers:{"Cache-Control":"private, no-store"}});
    if(p[0]==="directory-operations"){const {write}=await holdingResources(actor.holdingId);return Response.json(canonicalDestinationLinks({data:await directoryOperationsApi(write as PostgresDatabase,actor,p.slice(1))}),{headers:{"Cache-Control":"private, no-store"}});}
    if(p[0]==="templates"){const {write}=await holdingResources(actor.holdingId);return Response.json(canonicalDestinationLinks({data:await peopleTemplatesData(write as PostgresDatabase,iamActorFromWork(actor))}),{headers:{"Cache-Control":"private, no-store"}});}
    if(p[0]==="lifecycle" && p.length===1){const {write}=await holdingResources(actor.holdingId);return Response.json(canonicalDestinationLinks({data:await peopleLifecycleData(write,actor)}),{headers:{"Cache-Control":"private, no-store"}});}
    if(p[0]==="organization" && p[1]==="drafts"){const {write}=await holdingResources(actor.holdingId);return Response.json(canonicalDestinationLinks({data:await organizationDraftApi(write as PostgresDatabase,actor,p.slice(2))}),{headers:{"Cache-Control":"private, no-store"}});}
    if(p[0]==="organization" && p.length===1){requirePermission(actor,"people.manage");const {write}=await holdingResources(actor.holdingId);return Response.json(canonicalDestinationLinks({data:await organizationOverview(write,actor,Object.fromEntries(new URL(request.url).searchParams))}),{headers:{"Cache-Control":"private, no-store"}});}
    if(p[0]==="organization" && p[1]==="position-moves" && p.length<=3){const {write}=await holdingResources(actor.holdingId);return Response.json(canonicalDestinationLinks({data:await positionMoves(write,actor,p[2])}),{headers:{"Cache-Control":"private, no-store"}});}
    if(p[0]==="organization" && p[1]==="acceptance" && p.length===2){const {write}=await holdingResources(actor.holdingId);return Response.json(canonicalDestinationLinks({data:await companyBusinessAcceptance(write,actor)}),{headers:{"Cache-Control":"private, no-store"}});}
    if(p[0]==="organization" && p[1]==="readiness") {const {write}=await holdingResources(actor.holdingId);const kind=new URL(request.url).searchParams.get("kind");const route=await expenseRoute(write,actor.companyId,actor.userId,[],false,kind==="purchase"||kind==="payment"?kind:"expense");return Response.json(canonicalDestinationLinks({data:{ready:!route.blockedReason,blockedReason:route.blockedReason,steps:route.steps}}));}
    if (p.length === 0 || (p[0] === "people" && !p[1])) return Response.json(canonicalDestinationLinks({ data: await listPeople(actor) }), { headers: { "Cache-Control": "private, no-store" } });
    if (p[0] === "people" && p[1]) {
      const detail = await getPerson(actor, p[1]);
      const fullProfile = hasPermission(actor, "people.manage") || detail.person.user_id === actor.userId;
      return Response.json(canonicalDestinationLinks({ data: fullProfile ? detail : {
        ...detail,
        person: {
          id: detail.person.id,
          engagement_id: detail.person.engagement_id,
          engagement_version: detail.person.engagement_version,
          pay_group_id: detail.person.pay_group_id,
          currency: detail.person.currency,
        },
        leave: [], time: [], recommendedPay: null, recommendedLeave: null, leaveAssignment: null,
      } }), { headers: { "Cache-Control": "private, no-store" } });
    }
    if (p[0] === "pay-groups" && p[1] && p[2] === "schedule") return Response.json(canonicalDestinationLinks({data:await getPayrollSchedule(actor,p[1])}),{headers:{"Cache-Control":"private, no-store"}});
    if (p[0] === "pay-groups") return Response.json(canonicalDestinationLinks({data:await payrollOptions(actor)}));
    if (p[0] === "summary") return Response.json(canonicalDestinationLinks({ data: await peopleSummary(actor) }));
    if(p[0]==="payroll"&&p[1]==="payslips"){
      if(p.length===2)return Response.json(canonicalDestinationLinks({data:await listPayslips(actor)}),{headers:{"Cache-Control":"private, no-store"}});
      if(p.length===3)return Response.json(canonicalDestinationLinks({data:await getPayslip(actor,p[2])}),{headers:{"Cache-Control":"private, no-store"}});
      if(p.length===4&&p[3]==="export"){const issued=await getPayslipExport(actor,p[2]);return new Response(issued.html,{headers:{"Content-Type":"text/html; charset=utf-8","Content-Disposition":`attachment; filename="${p[2]}.html"`,"Cache-Control":"private, no-store","ETag":issued.checksum,"Content-Security-Policy":"default-src 'none'; style-src 'unsafe-inline'; frame-ancestors 'none'"}});}
    }
    if(p[0]==="payroll"&&p[1]==="runs"&&p[2]&&p[3]==="payslips"&&p.length===4)return Response.json(canonicalDestinationLinks({data:await listPayslips(actor,{runId:p[2],ownOnly:false})}),{headers:{"Cache-Control":"private, no-store"}});
    if (p[0] === "payroll" && p[1] === "summary" && p.length === 2) return Response.json(canonicalDestinationLinks({data:await payrollSummary(actor,new URL(request.url).searchParams.get("payMonth")??undefined)}),{headers:{"Cache-Control":"private, no-store"}});
    if (p[0] === "country-packs") return Response.json(canonicalDestinationLinks({ data: await listCountryPacks(actor) }));
    if (p[0] === "payroll" && p[1] === "thai-configuration") return Response.json(canonicalDestinationLinks({data:await thaiPayrollConfiguration(actor,new URL(request.url).searchParams.get("engagementId")??undefined)}),{headers:{"Cache-Control":"private, no-store"}});
    if (p[0] === "payroll" && p[1] === "runs" && p.length === 2) return Response.json(canonicalDestinationLinks({ data: await listPayrollRuns(actor) }));
    if (p[0] === "payroll" && p[1] === "posting-options" && p.length === 2) return Response.json(canonicalDestinationLinks({data:await payrollPostingOptions(actor)}),{headers:{"Cache-Control":"private, no-store"}});
    if (p[0] === "payroll" && p[1] === "cost-assignments" && p[2] && p.length === 3) return Response.json(canonicalDestinationLinks({data:await payrollCostAssignments(actor,p[2])}),{headers:{"Cache-Control":"private, no-store"}});
    if (p[0] === "payroll" && p[1] === "advances" && p.length === 2) return Response.json(canonicalDestinationLinks({ data: await payrollAdvancePage(actor) }));
    if (p[0] === "payroll" && p[1] === "recoveries" && p.length === 2) return Response.json(canonicalDestinationLinks({data:await payrollRecoveryPage(actor)}));
    if (p[0] === "payroll" && p[1] === "recoveries" && p[2] && p[3] === "agreement") {const document=await payrollRecoveryAgreement(actor,p[2]);return new Response(document.html,{headers:{'Content-Type':'text/html; charset=utf-8','Content-Disposition':`attachment; filename="payroll-recovery-${p[2]}.html"`,'X-Content-SHA256':document.checksum,'Cache-Control':'private, no-store'}});}
    if (p[0] === "payroll" && p[1] === "runs" && p[2] && p[3] === "review-file") {
      const query=new URL(request.url).searchParams,revision=Number(query.get("revision"));
      if(!Number.isInteger(revision)||revision<1)throw new ApiError(422,"REVISION_REQUIRED","Choose a payroll revision.");
      const result=await getPayrollReviewFile(actor,p[2],revision);
      return new Response(result.object.body,{status:200,headers:{"Content-Type":"text/csv; charset=utf-8","Content-Disposition":`attachment; filename="payroll-${p[2]}-review-${revision}.csv"`,"Cache-Control":"private, no-store","ETag":result.checksum}});
    }
    if (p[0] === "payroll" && p[1] === "runs" && p[2] && p[3] === "readiness") return Response.json(canonicalDestinationLinks({data:await payrollReadiness(actor,p[2])}),{headers:{"Cache-Control":"private, no-store"}});
    if(p[0]==='payroll'&&p[1]==='runs'&&['other-liabilities','other-remittances'].includes(p[3]??'')){
      if(p[2]&&p[3]==='other-liabilities'&&p.length===4){if(new URL(request.url).search)throw new ApiError(422,'INVALID_QUERY','This exact Payroll source reader accepts no query selectors.');return Response.json(canonicalDestinationLinks({data:await getPayrollOtherLiabilities(actor,p[2])}),{headers:{'Cache-Control':'private, no-store'}});}
      throw new ApiError(404,'NOT_FOUND','Payroll remittance resource not found.');
    }
    if (p[0] === "payroll" && p[1] === "runs" && p[2] && p[3] === "liabilities") return Response.json(canonicalDestinationLinks({data:await getPayrollLiabilities(actor,p[2])}),{headers:{"Cache-Control":"private, no-store"}});
    if (p[0] === "payroll" && p[1] === "runs" && p[2]) return Response.json(canonicalDestinationLinks({ data: await getPayrollRun(actor,p[2]) }));
    if (p[0] === "payments" && p[1] === "batches" && p[3] === "file") { const result=await getPaymentFile(actor,p[2]); const headers=new Headers({"Content-Type":"text/csv","Content-Disposition":`attachment; filename="${p[2]}.csv"`,"Cache-Control":"private, no-store"}); if(result.checksum)headers.set("ETag",result.checksum); return new Response(result.object.body,{status:200,headers}); }
    throw new ApiError(404,"NOT_FOUND","People resource not found.");
  } catch (error) { return errorResponse(error); }
}

export async function POST(request: Request, context: Context) {
  try {
    const origin=request.headers.get("origin"); if(origin && origin!==requestPublicOrigin(request)) throw new ApiError(403,"ORIGIN_FORBIDDEN","Use the application origin for this action.");
    const { holdingSlug, path } = await context.params; const actor = await requireWorkContext(holdingSlug); const p = parts(path); const input = await json(request); const id = command(request);
    if(p[0]==='payroll'&&p[1]?.startsWith('posting-prerequisites')){if(p[1]!=='posting-prerequisites'||p.length!==3)throw new ApiError(404,'NOT_FOUND','People resource not found.');if(p[2]==='confirmations')return Response.json(canonicalDestinationLinks({data:await payrollPostingPrerequisiteConfirmation(actor,parsed(payrollPostingPrerequisiteConfirmationSchema.safeParse(input)),id)}),{headers:{'Cache-Control':'private, no-store'}});const result=p[2]==='charts'?await createPayrollPostingChart(actor,parsed(payrollPostingChartSchema.safeParse(input)),id):p[2]==='accounts'?await createPayrollPostingAccount(actor,parsed(payrollPostingAccountSchema.safeParse(input)),id):p[2]==='periods'?await createPayrollPostingPeriod(actor,parsed(payrollPostingPeriodSchema.safeParse(input)),id):p[2]==='rates'?await confirmPayrollPostingRate(actor,parsed(payrollPostingRateSchema.safeParse(input)),id):null;if(!result)throw new ApiError(404,'NOT_FOUND','People resource not found.');return Response.json(canonicalDestinationLinks({data:result}),{status:201,headers:{'Cache-Control':'private, no-store'}});}
    if(p[0]==='payroll'&&p[1]==='runs'&&p[3]?.startsWith('final-pay-cancellation')){if(p[3]!=='final-pay-cancellation'||p.length!==4||!p[2])throw new ApiError(404,'NOT_FOUND','People resource not found.');return Response.json(canonicalDestinationLinks({data:await cancelFinalPayPreparation(actor,p[2],parsed(finalPayCancellationSchema.safeParse(input)),id)}),{headers:{'Cache-Control':'private, no-store'}});}
    if(p[0]==="payroll"&&p[1]==="runs"&&p[2]&&p[3]==="issue-payslips"&&p.length===4)return Response.json(canonicalDestinationLinks({data:await issuePostedPayslips(actor,p[2],parsed(issuePayslipSchema.safeParse(input)),id)}),{status:201});
    if(p[0]==="hire-handoffs"&&(p.length===1||(p.length===2&&p[1]==="consume")))return Response.json(canonicalDestinationLinks({data:await consumeHireHandoff(actor,parsed(hireConsumptionSchema.safeParse(input)),id)}),{status:201});
    if(p[0]==="directory-operations"){const {write}=await holdingResources(actor.holdingId);return Response.json(canonicalDestinationLinks({data:await directoryOperationsApi(write as PostgresDatabase,actor,p.slice(1),input,id)}));}
    if(p[0]==="templates"&&p.length===2&&["publish","preview","adopt"].includes(p[1])){const {write}=await holdingResources(actor.holdingId),iam=iamActorFromWork(actor);return Response.json(canonicalDestinationLinks({data:p[1]==="publish"?await saveHoldingTemplate(write as PostgresDatabase,iam,input,id):await adoptHoldingTemplate(write as PostgresDatabase,iam,parsed(adoptTemplateInput.safeParse(input)),id,p[1]==="preview")}));}
    if(p[0]==="lifecycle") {
      const {write}=await holdingResources(actor.holdingId),db=write as PostgresDatabase;
      if(p[1]==="placements" && p[2]==="batch" && p[3]==="preview")return Response.json(canonicalDestinationLinks({data:await previewPlacementBatch(db,actor,parsed(placementBatchInput.safeParse(input)))}),{headers:{"Cache-Control":"private, no-store"}});
      if(p[1]==="placements" && p[2]==="batch" && p[3]==="submit"){const {previewToken,...batch}=input;return Response.json(canonicalDestinationLinks({data:await submitPlacementBatch(db,actor,parsed(placementBatchInput.safeParse(batch)),parsed(z.string().length(64).safeParse(previewToken)),id)}),{headers:{"Cache-Control":"private, no-store"}});}
      if(input.supersedeLegacyCase!==undefined && !(p[1]==="placements" && p[2]==="submit"))throw new ApiError(422,"LEGACY_SUPERSEDE_INVALID","Confirm a legacy case only when submitting an HR placement.");
      if(p[1]==="packages" && p[2]==="preview")return Response.json(canonicalDestinationLinks({data:await previewPackage(db,actor,parsed(packageInput.safeParse(input)))}));
      if(p[1]==="packages" && ["draft","publish"].includes(p[2]))return Response.json(canonicalDestinationLinks({data:await savePackage(db,actor,parsed(packageInput.safeParse(input)),id,p[2]==="publish",input.previewToken)}));
      if(p[1]==="position" && ["preview","bind"].includes(p[2]))return Response.json(canonicalDestinationLinks({data:await bindPositionPackage(db,actor,parsed(z.object({positionId:z.string().min(1),packageId:z.string().min(1),revision:z.number().int().positive(),expectedVersion:z.number().int().positive(),previewToken:z.string().optional()}).safeParse(input)),id,p[2]==="preview")}));
      if(p[1]==="placements" && p[2]==="preview")return Response.json(canonicalDestinationLinks({data:await previewLifecycle(db,actor,parsed(lifecycleInput.safeParse(input)),true)}));
      if(p[1]==="placements" && ["draft","submit"].includes(p[2]))return Response.json(canonicalDestinationLinks({data:await saveLifecycle(db,actor,parsed(lifecycleInput.safeParse(input)),id,p[2]==="submit",input.previewToken,true,input.supersedeLegacyCase===undefined?undefined:parsed(legacySupersedeInput.safeParse(input.supersedeLegacyCase)))}));
      if(p[1]==="placements" && p[3]==="cancel")return Response.json(canonicalDestinationLinks({data:await cancelLifecycle(db,actor,p[2],parsed(z.object({expectedVersion:z.number().int().positive()}).safeParse(input)).expectedVersion,id)}));
      if(p[1]==="cases" && p[2]==="preview")return Response.json(canonicalDestinationLinks({data:await previewLifecycle(db,actor,parsed(lifecycleInput.safeParse(input)))}));
      if(p[1]==="cases" && ["draft","submit"].includes(p[2]))return Response.json(canonicalDestinationLinks({data:await saveLifecycle(db,actor,parsed(lifecycleInput.safeParse(input)),id,p[2]==="submit",input.previewToken)}));
      if(p[1]==="cases" && ["cancel","request-access"].includes(p[3])) {
        const v=parsed(z.object({expectedVersion:z.number().int().positive()}).safeParse(input));
        return Response.json(canonicalDestinationLinks({data:p[3]==="cancel"?await cancelLifecycle(db,actor,p[2],v.expectedVersion,id):await requestLifecycleAccess(db,actor,p[2],v.expectedVersion,id)}));
      }
      if(p[1]==="rehire")return Response.json(canonicalDestinationLinks({data:await createRehireEngagement(db,actor,parsed(rehireInput.safeParse(input)),id)}));
      if(p[1]==="settings")return Response.json(canonicalDestinationLinks({data:await saveLifecycleSettings(db,actor,parsed(lifecycleSettingsInput.safeParse(input)),id)}));
      if(p[1]==="tasks" && p[3]==="complete")return Response.json(canonicalDestinationLinks({data:await completePreparationTask(db,actor,p[2],parsed(z.object({expectedVersion:z.number().int().positive()}).safeParse(input)).expectedVersion,id)}));
      if(p[1]==="tasks" && p[3]==="final-pay")return Response.json(canonicalDestinationLinks({data:await prepareFinalPay(actor,parsed(finalPayPreparationSchema.safeParse(input)),id)}));
    }
    if(p[0]==="organization") {
      if(p[1]==="drafts"){const {write}=await holdingResources(actor.holdingId);return Response.json(canonicalDestinationLinks({data:await organizationDraftApi(write as PostgresDatabase,actor,p.slice(2),input,id)}));}
      const {write}=await holdingResources(actor.holdingId);
      if(p[1]==="changes"&&p[2]==="preview"&&p.length===3)return Response.json(canonicalDestinationLinks({data:await previewOrganizationNodeChange(write,actor,parsed(organizationNodeChangeSchema.safeParse(input)))}));
      if(p[1]==="position-moves" && p[2]==="preview" && p.length===3)return Response.json(canonicalDestinationLinks({data:await previewPositionMove(write as PostgresDatabase,actor,parsed(positionMoveInput.safeParse(input)))}));
      if(p[1]==="position-moves" && p[2]==="submit" && p.length===3){const data=parsed(positionMoveSubmitInput.safeParse(input));const {previewToken,expiresAt,...move}=data;return Response.json(canonicalDestinationLinks({data:await submitPositionMove(write as PostgresDatabase,actor,move,previewToken,expiresAt,id)}));}
      if(p[1]==="position-moves" && p[2] && p[3]==="cancel" && p.length===4)return Response.json(canonicalDestinationLinks({data:await cancelPositionMove(write as PostgresDatabase,actor,p[2],parsed(z.object({expectedVersion:z.number().int().positive()}).safeParse(input)).expectedVersion,id)}));
      if(p[1]==="positions" && p[2] && p[3]==="default-pay")return Response.json(canonicalDestinationLinks({data:await savePositionPayDefault(write,actor,p[2],parsed(positionPayInput.safeParse(input)),id)}));
      if(p[1]==="levels" && p[2] && p[3]==="default-leave")return Response.json(canonicalDestinationLinks({data:await saveLevelLeaveDefault(write,actor,p[2],parsed(levelLeaveInput.safeParse(input)),id)}));
      if(p[1]==="spending-template")return Response.json(canonicalDestinationLinks({data:await saveSpendingTemplate(write,actor,parsed(spendingTemplateSchema.safeParse(input)),id)}));
      if(p[1]==="acceptance" && p[2]==="submit") return Response.json(canonicalDestinationLinks({data:await submitCompanyBusinessAcceptance(write,actor,parsed(companyAcceptanceSubmitSchema.safeParse(input)),id)}));
      if(p[1]==="acceptance" && p[2]==="review") return Response.json(canonicalDestinationLinks({data:await decideCompanyBusinessAcceptance(write,actor,parsed(companyAcceptanceDecisionSchema.safeParse(input)),id)}));
      if(p[1]==="acceptance" && p[2]==="revoke") return Response.json(canonicalDestinationLinks({data:await revokeCompanyBusinessAcceptance(write,actor,parsed(companyAcceptanceRevokeSchema.safeParse(input)),id)}));
      if(p[1]==="levels" && p.length===2)return Response.json(canonicalDestinationLinks({data:await installStaffLevels(write,actor,id)}));
      if(p[1]==="changes"&&["draft","submit"].includes(p[2])&&p.length===3)return Response.json(canonicalDestinationLinks({data:await saveOrganizationNodeChange(write,actor,null,parsed(organizationNodeChangeSchema.safeParse(input)),id,p[2]==="submit")}));
      if(p[1]==="changes"&&p[2]&&["draft","submit"].includes(p[3]))return Response.json(canonicalDestinationLinks({data:await saveOrganizationNodeChange(write,actor,p[2],parsed(organizationNodeChangeSchema.safeParse(input)),id,p[3]==="submit")}));
      if(p[1]==="changes"&&p[2]&&p[3]==="review")return Response.json(canonicalDestinationLinks({data:await decideOrganizationNodeChange(write,actor,p[2],parsed(organizationNodeDecisionSchema.safeParse(input)),id)}));
      if(p[1]==="changes"&&p[2]&&p[3]==="cancel"){const data=parsed(z.object({expectedVersion:z.number().int().positive()}).safeParse(input));return Response.json(canonicalDestinationLinks({data:await cancelOrganizationNodeChange(write,actor,p[2],data.expectedVersion,id)}));}
      if(p[1]==="positions" && p.length<=3)return Response.json(canonicalDestinationLinks({data:await savePosition(write,actor,p[2]??null,parsed(positionSchema.safeParse(input)),id)}));
      if(p[1]==="templates" && p[3]==="apply")return Response.json(canonicalDestinationLinks({data:await applyOrganizationTemplate(write,actor,p[2],parsed(applyOrganizationTemplateSchema.safeParse(input)),id)}));
      if(p[1]==="templates" && p.length<=3)return Response.json(canonicalDestinationLinks({data:await saveOrganizationTemplate(write,actor,p[2]??null,parsed(organizationTemplateSchema.safeParse(input)),id)}));
    }
    if(p[0]==="directory"&&p[2]==="invite")return Response.json(canonicalDestinationLinks({data:await invitePerson(actor,p[1],parsed(grantSchema.safeParse(input)),id)}));
    if(p[0]==="directory"&&p[2]==="invitations"&&p[3]&&p[4]==="retry")return Response.json(canonicalDestinationLinks({data:await retryInvitationDelivery(actor,p[1],p[3],id)}));
    if(p[0]==="directory"&&p[2]==="invitations"&&p[3]&&p[4]==="revoke")return Response.json(canonicalDestinationLinks({data:await revokeInvitation(actor,p[1],p[3],id)}));
    if(p[0]==="directory"&&p[2]==="grant")return Response.json(canonicalDestinationLinks({data:await grantPersonAccess(actor,p[1],parsed(grantSchema.safeParse(input)),id)}));
    if(p[0]==="directory"&&p[2]==="revoke"){const data=parsed(z.object({assignmentId:z.string().min(1)}).safeParse(input));return Response.json(canonicalDestinationLinks({data:await revokePersonAccess(actor,p[1],data.assignmentId,id)}));}
    if (p[0] === "directory" && p.length === 1) return Response.json(canonicalDestinationLinks({data:await saveDirectoryPerson(actor,null,{...parsed(directorySchema.safeParse(input)),expectedVersion:1},id)}),{status:201});
    if (p[0] === "directory" && p.length === 2) return Response.json(canonicalDestinationLinks({data:await saveDirectoryPerson(actor,p[1],parsed(directoryUpdateSchema.safeParse(input)),id)}));
    if (p[0] === "directory" && p[2] === "status") return Response.json(canonicalDestinationLinks({data:await setDirectoryStatus(actor,p[1],parsed(directoryStatusSchema.safeParse(input)),id)}));
    if (p[0] === "people" && p.length === 1) { const data=parsed(createPersonSchema.safeParse(input)); return Response.json(canonicalDestinationLinks({data:await createPerson(actor,data,id)}),{status:201}); }
    if (p[0] === "engagements" && p.length === 1) { const data=parsed(createEngagementSchema.safeParse(input)); return Response.json(canonicalDestinationLinks({data:await createEngagement(actor,data,id)}),{status:201}); }
    if(p[0]==="engagements"&&p[2]==="pay-group")return Response.json(canonicalDestinationLinks({data:await assignPayGroup(actor,p[1],parsed(assignPayGroupSchema.safeParse(input)),id)}));
    if (p[0] === "engagements" && p[2] === "transition") { const data=parsed(engagementTransitionSchema.safeParse(input)); return Response.json(canonicalDestinationLinks({data:await transitionEngagement(actor,p[1],data,id)})); }
    if (p[0] === "compensation" && p.length === 1) { const data=parsed(compensationSchema.safeParse(input)); return Response.json(canonicalDestinationLinks({data:await createCompensation(actor,data,id)}),{status:201}); }
    if (p[0] === "compensation" && p[2] === "transition") { const data=parsed(compensationTransitionSchema.safeParse(input)); return Response.json(canonicalDestinationLinks({data:await transitionCompensation(actor,p[1],data,id)})); }
    if (p[0] === "pay-groups" && p.length === 1) { const data=parsed(payGroupSchema.safeParse(input)); return Response.json(canonicalDestinationLinks({data:await createPayGroup(actor,data,id)}),{status:201}); }
    if (p[0] === "pay-groups" && p[1] && p[2] === "schedule") return Response.json(canonicalDestinationLinks({data:await savePayrollSchedule(actor,p[1],parsed(payrollScheduleSchema.safeParse(input)),id)}));
    if (p[0] === "payroll" && p[1] === "thai-profiles" && p.length === 2) return Response.json(canonicalDestinationLinks({data:await createThaiPayrollProfile(actor,parsed(thaiPayrollProfileSchema.safeParse(input)),id)}),{status:201});
    if (p[0] === "payroll" && p[1] === "thai-profiles" && p[3] === "decision") return Response.json(canonicalDestinationLinks({data:await decideThaiPayrollProfile(actor,p[2],parsed(thaiPayrollProfileDecisionSchema.safeParse(input)),id)}));
    if (p[0] === "country-packs" && p.length === 1) { const data=parsed(countryPackSchema.safeParse(input)); return Response.json(canonicalDestinationLinks({data:await createCountryPack(actor,data,id)}),{status:201}); }
    if (p[0] === "country-packs" && p[2] === "transition") { const data=parsed(countryPackTransitionSchema.safeParse(input)); return Response.json(canonicalDestinationLinks({data:await transitionCountryPack(actor,p[1],data,id)})); }
    if (p[0] === "expenses") throw new ApiError(410,"EXPENSE_API_MOVED","Use the /expenses workflow for claims, receipts, approval and reimbursement.");
    if (p[0] === "time-entries" && p.length === 1) { const data=parsed(timeEntrySchema.safeParse(input)); return Response.json(canonicalDestinationLinks({data:await createTimeEntry(actor,data,id)}),{status:201}); }
    if (p[0] === "time-entries" && p.length === 2) return Response.json(canonicalDestinationLinks({data:await updateTimeEntry(actor,p[1],parsed(timeEntryUpdateSchema.safeParse(input)),id)}));
    if (p[0] === "time-entries" && p[2] === "reject") return Response.json(canonicalDestinationLinks({data:await rejectTimeEntry(actor,p[1],parsed(timeEntryReviewSchema.safeParse(input)),id)}));
    if (p[0] === "time-entries" && p[2] === "approve") { const data=parsed(z.object({expectedVersion:z.number().int().positive()}).safeParse(input)); return Response.json(canonicalDestinationLinks({data:await approveTimeEntry(actor,p[1],data.expectedVersion,id)})); }
    if (p[0] === "payroll" && p[1] === "runs" && p.length === 2) { const data=parsed(payrollRunSchema.safeParse(input)); return Response.json(canonicalDestinationLinks({data:await createPayrollRun(actor,data,id)}),{status:201}); }
    if (p[0] === "payroll" && p[1] === "pay-groups" && p[2] && p[3] === "account-mappings") return Response.json(canonicalDestinationLinks({data:await savePayrollAccountMapping(actor,p[2],parsed(payrollAccountMappingSchema.safeParse(input)),id)}));
    if (p[0] === "payroll" && p[1] === "cost-assignments" && p[2] && p[3] === "verify") return Response.json(canonicalDestinationLinks({data:await verifyPayrollCostAssignment(actor,p[2],parsed(payrollCostVerificationSchema.safeParse(input)),id)}));
    if (p[0] === "payroll" && p[1] === "account-mappings" && p[2] && p[3] === "approve") { const data=parsed(z.object({expectedVersion:z.number().int().positive()}).safeParse(input)); return Response.json(canonicalDestinationLinks({data:await approvePayrollAccountMapping(actor,p[2],data.expectedVersion,id)})); }
    if (p[0] === "payroll" && p[1] === "runs" && p[3] === "calculate") return Response.json(canonicalDestinationLinks({data:await calculatePayrollRun(actor,p[2],id)}));
    if (p[0] === "payroll" && p[1] === "runs" && p[3] === "cancel" && p.length === 4) return Response.json(canonicalDestinationLinks({data:await cancelPayrollRun(actor,p[2],parsed(payrollCancelSchema.safeParse(input)),id)}));
    if (p[0] === "payroll" && p[1] === "runs" && p[3] === "adjustments" && p.length === 4) return Response.json(canonicalDestinationLinks({data:await createPayrollAdjustment(actor,p[2],parsed(payrollAdjustmentCreateSchema.safeParse(input)),id)}),{status:201});
    if (p[0] === "payroll" && p[1] === "runs" && p[3] === "adjustment-edit" && p.length === 4) return Response.json(canonicalDestinationLinks({data:await editPayrollAdjustment(actor,p[2],parsed(payrollAdjustmentEditSchema.safeParse(input)),id)}));
    if (p[0] === "payroll" && p[1] === "runs" && p[3] === "recoveries" && p.length === 4) return Response.json(canonicalDestinationLinks({data:await requestPayrollRecovery(actor,p[2],parsed(payrollRecoveryRequestSchema.safeParse(input)),id)}),{status:201});
    if (p[0] === "payroll" && p[1] === "recoveries" && p[2] && p[3] === "consent") return Response.json(canonicalDestinationLinks({data:await consentPayrollRecovery(actor,p[2],parsed(payrollRecoveryConsentSchema.safeParse(input)),id)}));
    if (p[0] === "payroll" && p[1] === "recoveries" && p[2] && p[3] === "review") return Response.json(canonicalDestinationLinks({data:await reviewPayrollRecovery(actor,p[2],parsed(payrollRecoveryReviewSchema.safeParse(input)),id)}));
    if (p[0] === "payroll" && p[1] === "recoveries" && p[2] && p[3] === "recognize") return Response.json(canonicalDestinationLinks({data:await recognizePayrollRecovery(actor,p[2],parsed(payrollRecoveryRecognizeSchema.safeParse(input)),id)}));
    if (p[0] === "payroll" && p[1] === "runs" && p[3] === "submit-review") return Response.json(canonicalDestinationLinks({data:await submitPayrollReview(actor,p[2],parsed(payrollSubmitReviewSchema.safeParse(input)),id)}));
    if(p[0]==='payroll'&&p[1]==='runs'&&['other-liabilities','other-remittances'].includes(p[3]??'')){
      if(p[2]&&p[3]==='other-remittances'&&p.length===4)return Response.json(canonicalDestinationLinks({data:await remitPayrollOtherLiability(actor,p[2],parsed(payrollOtherRemittanceSchema.safeParse(input)),id)}),{headers:{'Cache-Control':'private, no-store'}});
      if(p[2]&&p[3]==='other-remittances'&&p[4]==='bank-evidence'&&p[5]==='ignore'&&p.length===6)return Response.json(canonicalDestinationLinks({data:await ignorePayrollOtherLiabilityBankEvidence(actor,p[2],parsed(payrollOtherBankEvidenceIgnoreSchema.safeParse(input)),id)}),{headers:{'Cache-Control':'private, no-store'}});
      if(p[2]&&p[3]==='other-remittances'&&p[4]==='bank-evidence'&&p.length===5)return Response.json(canonicalDestinationLinks({data:await importPayrollOtherLiabilityBankEvidence(actor,p[2],parsed(payrollOtherBankEvidenceSchema.safeParse(input)),id)}),{headers:{'Cache-Control':'private, no-store'}});
      throw new ApiError(404,'NOT_FOUND','Payroll remittance resource not found.');
    }
    if (p[0] === "payroll" && p[1] === "runs" && p[3] === "remit") return Response.json(canonicalDestinationLinks({data:await remitPayrollLiability(actor,p[2],parsed(payrollRemittanceSchema.safeParse(input)),id)}));
    if (p[0] === "payroll" && p[1] === "runs" && p[3] === "approve") { const data=parsed(z.object({expectedVersion:z.number().int().positive()}).safeParse(input)); return Response.json(canonicalDestinationLinks({data:await approvePayrollRun(actor,p[2],data.expectedVersion,id)})); }
    if (p[0] === "payroll" && p[1] === "runs" && p[3] === "corrections" && p.length===4) return Response.json(canonicalDestinationLinks({data:await requestPayrollCorrection(actor,p[2],parsed(payrollCorrectionRequestSchema.safeParse(input)),id)}));
    if (p[0] === "payroll" && p[1] === "corrections" && p[2] && p[3] === "decide") return Response.json(canonicalDestinationLinks({data:await decidePayrollCorrection(actor,p[2],parsed(payrollCorrectionDecisionSchema.safeParse(input)),id)}));
    if (p[0] === "payroll" && p[1] === "runs" && p[3] === "export-review") return Response.json(canonicalDestinationLinks({data:await exportPayrollReview(actor,p[2],id)}));
    if (p[0] === "payroll" && p[1] === "runs" && p[3] === "accounting-preview") return Response.json(canonicalDestinationLinks({data:await previewPayrollAccounting(actor,p[2],parsed(payrollPostSchema.safeParse(input)))}));
    if (p[0] === "payroll" && p[1] === "runs" && p[3] === "post") { const data=parsed(payrollPostSchema.safeParse(input)); return Response.json(canonicalDestinationLinks({data:await postPayroll(actor,p[2],data,id)})); }
    if (p[0] === "payroll" && p[1] === "advances" && p.length === 2) return Response.json(canonicalDestinationLinks({data:await requestPayrollAdvance(actor,parsed(payrollAdvanceRequestSchema.safeParse(input)),id)}),{status:201});
    if (p[0] === "payroll" && p[1] === "advances" && p[2] && p[3] === "decide") return Response.json(canonicalDestinationLinks({data:await decidePayrollAdvance(actor,p[2],parsed(payrollAdvanceDecisionSchema.safeParse(input)),id)}));
    if (p[0] === "payments" && p[1] === "batches" && p.length === 2) { const data=parsed(paymentBatchSchema.safeParse(input)); return Response.json(canonicalDestinationLinks({data:await createPaymentBatch(actor,data,id)}),{status:201}); }
    if (p[0] === "payments" && p[1] === "batches" && p[3] === "retry") { const data=parsed(z.object({expectedVersion:z.number().int().positive()}).strict().safeParse(input)); return Response.json(canonicalDestinationLinks({data:await preparePaymentRetry(actor,p[2],data.expectedVersion,id)})); }
    if (p[0] === "payments" && p[1] === "batches" && p[3] === "approve") { const data=parsed(z.object({expectedVersion:z.number().int().positive()}).safeParse(input)); return Response.json(canonicalDestinationLinks({data:await approvePaymentBatch(actor,p[2],data.expectedVersion,id)})); }
    if (p[0] === "payments" && p[1] === "batches" && p[3] === "export") return Response.json(canonicalDestinationLinks({data:await exportPaymentBatch(actor,p[2],id)}));
    if (p[0] === "payments" && p[1] === "batches" && p[3] === "confirm") { const data=parsed(paymentConfirmationSchema.safeParse(input)); return Response.json(canonicalDestinationLinks({data:await confirmPaymentBatch(actor,p[2],data,id)})); }
    if (p[0] === "payments" && p[1] === "batches" && p[3] === "import-result") { const data=parsed(z.object({csv:z.string().min(1).max(2_000_000)}).strict().safeParse(input)); return Response.json(canonicalDestinationLinks({data:await importSyntheticPaymentResults(actor,p[2],data.csv,id)})); }
    throw new ApiError(404,"NOT_FOUND","People resource not found.");
  } catch (error) { return errorResponse(error); }
}
