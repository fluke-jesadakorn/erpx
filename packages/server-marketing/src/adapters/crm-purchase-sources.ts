import "server-only";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {marketingPointExistsSql} from './marketing-point-exists-sql';
import {marketingSelectHeadSql} from './select-head-sql';
/** Captured form identity and immutable submission event survive later form
 * retirement; a supplied source label is never a candidate handoff witness. */
export const crmMarketingCandidateSources=marketingSelectHeadSql(`candidate.* FROM marketing_lead_candidates candidate
 JOIN marketing_assets form ON form.id=candidate.form_id AND form.company_id=candidate.company_id AND form.kind='form' AND form.version>=candidate.form_version
 JOIN marketing_asset_events captured ON captured.asset_id=form.id AND captured.version=candidate.form_version AND captured.action='activate' AND captured.status='active'
 JOIN marketing_engagement_events submission ON submission.candidate_id=candidate.id AND submission.version=1 AND submission.event_type='form_submit'
 WHERE length(trim(candidate.consent_evidence))>=3`,'');
export const crmMarketingCampaignSources=marketingSelectHeadSql('id,company_id',' FROM marketing_campaigns');
/** One fixed owner projection for the retained public-submission receipt
 * witness. The purchase path and the public-form load execute this exact
 * statement inside the caller's Holding transaction; a single source keeps the
 * consent/receipt check from drifting between them. Callers supply only the
 * key, Company, form id and form version as positional binds. */
export const publicSubmissionReceiptWitness=marketingPointExistsSql('marketing_public_receipts receipt JOIN ('+crmMarketingCandidateSources+') candidate ON candidate.id=receipt.candidate_id','receipt.key=? AND candidate.company_id=? AND candidate.form_id=? AND candidate.form_version=?');
export function crmMarketingSourceReadLocks(database:SqlDatabase){return [database.prepare("LOCK TABLE marketing_assets,marketing_asset_events,marketing_lead_candidates,marketing_engagement_events,marketing_campaigns,marketing_touches IN SHARE ROW EXCLUSIVE MODE")];}
