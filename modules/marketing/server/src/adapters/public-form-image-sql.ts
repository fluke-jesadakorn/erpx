import 'server-only';
import { marketingSelectHeadSql } from './select-head-sql';

/** Closed-variant owner-local image point reads for the public Marketing form.
 * The three admitted snapshot queries stay byte-identical to the original
 * reader: a caller picks a variant and never supplies raw query text. Table,
 * projection, joins, filter and bind positions stay owned here. r46: the head
 * renders through the owner-local head renderer. */
type ImageVariant = 'form' | 'content' | 'receipt';

const imageParts: Record<ImageVariant, { projection: string; source: string }> = {
  form: {
    projection: "company_id,status,configuration_json,to_jsonb(form)::text image",
    source: " FROM marketing_assets form WHERE id=? AND kind='form'",
  },
  content: {
    projection: "to_jsonb(content)::text image",
    source: " FROM marketing_assets content WHERE id=? AND company_id=? AND kind='content'",
  },
  receipt: {
    projection:
      "jsonb_build_array(to_jsonb(receipt),to_jsonb(candidate),to_jsonb(activated),to_jsonb(submission))::text image",
    source: ` FROM marketing_public_receipts receipt
 JOIN marketing_lead_candidates candidate ON candidate.id=receipt.candidate_id AND candidate.company_id=? AND candidate.form_id=?
 JOIN marketing_asset_events activated ON activated.asset_id=candidate.form_id AND activated.version=candidate.form_version AND activated.action='activate' AND activated.status='active'
 JOIN marketing_engagement_events submission ON submission.candidate_id=candidate.id AND submission.version=1 AND submission.event_type='form_submit'
 WHERE receipt.key=?`,
  },
};

export function publicFormImageSql(variant: ImageVariant): string {
  const { projection, source } = imageParts[variant];
  return marketingSelectHeadSql(projection, source);
}
