import type { WorkItem } from "@/lib/work-item";
import { hasPermission } from "@/lib/permissions";
import { samePerson } from "@/lib/iam";
import type { WorkContext } from "./authorization";
import { requirePermission } from "./iam-permissions";
import { holdingResources } from "./runtime";
import {financeExpansionClosePending} from "./finance-expansion-close";
export async function financeExpansionWork(
  actor: WorkContext,
): Promise<WorkItem[]> {
  actor = requirePermission(actor, "finance.read");
  const { write } = await holdingResources(actor.holdingId);
  const rows = (
    await write
      .prepare(
        `SELECT id,name title,kind,chart_id,'draft' status,prepared_by,created_at updated_at FROM finance_plan_editions WHERE company_id=? AND status='draft'
 UNION ALL SELECT id,'Book opening and mapping','book',secondary_chart_id,status,prepared_by,created_at FROM finance_book_rules WHERE company_id=? AND status='draft'
 UNION ALL SELECT id,'Bank match evidence','bank',NULL,status,proposed_by,updated_at FROM finance_bank_match_groups WHERE company_id=? AND status='proposed'
 UNION ALL SELECT id,'Customer collection message','collection',NULL,status,prepared_by,created_at FROM finance_collection_cases WHERE company_id=? AND status IN ('draft','approved')
 UNION ALL SELECT id,name,'contract',chart_id,status,prepared_by,created_at FROM finance_business_contracts WHERE company_id=? AND status='draft'
 UNION ALL SELECT id,'Posted billing credit and replacement','billing-correction',NULL,status,prepared_by,updated_at FROM finance_billing_corrections WHERE company_id=? AND status='draft'
 UNION ALL SELECT b.id,'Zero-usage service evidence','zero-usage',c.chart_id,b.no_charge_status,b.created_by,b.created_at FROM finance_business_billing_runs b JOIN finance_business_contracts c ON c.id=b.contract_id WHERE b.company_id=? AND b.no_charge_status='draft' AND NOT EXISTS(SELECT 1 FROM finance_business_billing_runs child WHERE child.parent_run_id=b.id)
 UNION ALL SELECT r.id,'Service revenue recognition','recognition',c.chart_id,r.status,r.prepared_by,r.updated_at FROM finance_recognition_runs r JOIN finance_business_billing_runs b ON b.id=r.billing_run_id JOIN finance_business_contracts c ON c.id=b.contract_id WHERE r.company_id=? AND r.status='draft'
 UNION ALL SELECT id,'Secondary book policy adjustment','book-adjustment',chart_id,status,prepared_by,created_at FROM finance_book_adjustments WHERE company_id=? AND status='draft'
 UNION ALL SELECT id,name,'instrument',chart_id,status,prepared_by,created_at FROM finance_instrument_editions WHERE company_id=? AND status='draft'
 UNION ALL SELECT r.id,'Financing bank payment','instrument-run',e.chart_id,r.status,r.prepared_by,r.created_at FROM finance_instrument_runs r JOIN finance_instrument_editions e ON e.id=r.instrument_id WHERE r.company_id=? AND r.status='draft'
 UNION ALL SELECT id,'Asset depreciation policy','asset-policy',NULL,status,prepared_by,created_at FROM finance_asset_policy_editions WHERE company_id=? AND status='draft'
 UNION ALL SELECT id,'Asset depreciation review','asset-policy-run',NULL,status,prepared_by,created_at FROM finance_asset_policy_runs WHERE company_id=? AND status='draft'
 UNION ALL SELECT id,'Group reporting edition','group-edition',NULL,status,prepared_by,created_at FROM finance_group_report_editions WHERE company_id=? AND status='draft'
 UNION ALL SELECT id,'Ownership and control evidence','ownership',NULL,status,created_by,created_at FROM finance_ownership_interests WHERE company_id=? AND status='draft' AND source_hash IS NOT NULL
 UNION ALL SELECT id,'Acquisition and goodwill evidence','acquisition',NULL,status,created_by,created_at FROM finance_acquisitions WHERE company_id=? AND status='draft' AND source_hash IS NOT NULL
 UNION ALL SELECT id,'Intercompany document pair','intercompany-pair',NULL,status,prepared_by,created_at FROM finance_intercompany_document_pairs WHERE company_id=? AND status='draft'
 ORDER BY updated_at DESC,id LIMIT 500`,
      )
      .bind(...Array.from({ length: 17 }, () => actor.companyId))
      .all<{
        id: string;
        title: string;
        kind: string;
        chart_id: string | null;
        status: string;
        prepared_by: string;
        updated_at: number;
      }>()
  ).results;
  const due = actor.scope.kind === "holding" ? (await write.prepare("SELECT id,kind,run_on,status,last_message,updated_at FROM finance_due_drafts WHERE company_id=? AND status IN ('scheduled','blocked') ORDER BY run_on,id LIMIT 500").bind(actor.companyId).all<{id:string;kind:string;run_on:string;status:string;last_message:string|null;updated_at:number}>()).results.map((r):WorkItem=>({id:r.id,type:"finance_review",title:`Scheduled ${r.kind.replaceAll('_',' ')} draft`,detail:r.last_message??`Prepare on ${r.run_on} with retained source evidence`,href:"/finance/due-drafts",status:r.status,updatedAt:r.updated_at,queue:r.status==='blocked'&&hasPermission(actor,'finance.write')?'todo':'waiting',canAct:r.status==='blocked'&&hasPermission(actor,'finance.write'),actionKind:"work",nextAction:r.status==='blocked'?'Resolve the source issue and retry':'Waiting for the scheduled date',responsibleQueue:"Company Finance",relationships:["team"]})) : [];
  const today=new Intl.DateTimeFormat("en-CA",{timeZone:"Asia/Bangkok",year:"numeric",month:"2-digit",day:"2-digit"}).format(new Date()),periods=actor.scope.kind==='holding'?(await write.prepare("SELECT id,name,start_date,end_date FROM finance_periods WHERE company_id=? AND state IN ('open','closing') AND end_date<=? ORDER BY start_date,id LIMIT 6").bind(actor.companyId,today).all<{id:string;name:string;start_date:string;end_date:string}>()).results:[],omitted:WorkItem[]=[];
  for(const period of periods){const missing=await financeExpansionClosePending(write,actor.companyId,period.start_date,period.end_date);for(const r of missing.filter(r=>r.kind.startsWith('due '))){const href=r.kind.includes('asset')?'/finance/asset-policies':r.kind.includes('financing')?'/finance/instruments':'/finance/contracts',canAct=hasPermission(actor,'finance.write');omitted.push({id:`finance-due-source:${period.id}:${r.kind}:${r.id}`,type:'finance_review',title:`Prepare ${r.kind.slice(4)}`,detail:`${period.name}: review the source evidence and prepare the missing draft before closing this period.`,href,status:'source_missing',updatedAt:Date.parse(period.end_date),queue:canAct?'todo':'waiting',canAct,actionKind:'work',nextAction:'Prepare the source draft',responsibleQueue:'Company Finance',relationships:['team']});}}
  return [...omitted,...due, ...rows.filter((r) => actor.scope.kind === "holding" || !["group-edition", "ownership", "acquisition", "intercompany-pair"].includes(r.kind)).map((r) => {
    const self = samePerson(actor, r.prepared_by),
      delivery = r.kind === "collection" && r.status === "approved",
      canAct =
        [
          "group-edition",
          "ownership",
          "acquisition",
          "intercompany-pair",
        ].includes(r.kind) && actor.scope.kind !== "holding"
          ? false
          : delivery
            ? hasPermission(actor, "finance.write")
            : !self && hasPermission(actor, "finance.approve"),
      href = [
        "group-edition",
        "ownership",
        "acquisition",
        "intercompany-pair",
      ].includes(r.kind)
        ? "/finance/group-editions"
        : r.kind.startsWith("instrument")
          ? "/finance/instruments"
          : r.kind.startsWith("asset-policy")
            ? "/finance/asset-policies"
            : r.kind === "book-adjustment"
              ? "/finance/book-adjustments"
                : ["contract", "recognition", "zero-usage", "billing-correction"].includes(r.kind)
                ? "/finance/contracts"
                : r.kind === "book"
                  ? "/finance/setup?step=books"
                  : r.kind === "bank"
                    ? "/finance/bank"
                    : r.kind === "collection"
                      ? "/finance/collections"
                      : `/finance/reports?chart=${encodeURIComponent(r.chart_id ?? "")}`;
    return {
      id: r.id,
      type: "finance_review",
      title: r.title,
      detail: delivery
        ? "Reviewed message ready for test delivery"
        : `Review ${r.kind} source evidence before approval`,
      href,
      status: r.status,
      updatedAt: r.updated_at,
      queue: canAct ? "todo" : "waiting",
      canAct,
      actionKind: delivery ? "work" : "approval",
      nextAction: delivery
        ? "Record test delivery"
        : self
          ? "Waiting for an independent Finance approver"
          : "Review and approve",
      responsibleQueue: "Company Finance",
      relationships: self
        ? ["requested", "team"]
        : canAct
          ? ["approval", "team"]
          : ["team"],
    } satisfies WorkItem;
  })];
}
