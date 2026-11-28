import "server-only";
/** Closed native projections for commercial owner joins. Callers retain their original scope/authority filters and execute the composed statement in their supplied transaction. No SQL or relation name is accepted from a caller. */
/** One parameterized renderer for CRM closed-projection column lists. Every call
 * site renders byte-identical SQL from a fixed table and fixed column tuple;
 * no SQL or relation name is accepted from a caller. */
export const crmProjectionSql=(table:string,columns:readonly string[],separator=',')=>`SELECT ${columns.map(column=>`"${column}"`).join(separator)} FROM ${table}`;
export const crmLeadColumns=['id','company_id','branch_id','department_id','title','contact_name','contact_detail','source','brief','owner_id','due_date','status','version','created_by','created_at','updated_at','account_id','contact_id'] as const;
export const crmLeadActivityColumns=['id','company_id','lead_id','activity_type','subject','details','owner_id','due_date','status','outcome','version','created_by','created_at','updated_at'] as const;
export const crmCaseEventColumns=['id','lead_id','opportunity_id','version','action','actor_id','reason','snapshot_json','created_at'] as const;
export const crmOpportunityColumns=['id','lead_id','customer_id','stage','expected_minor','currency','close_date','quote_id','quote_version','sales_id','version','created_by','created_at','updated_at','reservation_id','win_source','won_at'] as const;
export const commercialOwnerProjectionSources = {
  crm_leads: crmProjectionSql('crm_leads',crmLeadColumns),
} as const;
