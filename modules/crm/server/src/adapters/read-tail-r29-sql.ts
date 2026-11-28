/** G4 / retire.crm.read-tail-r29 byte-identical read families.
 *
 * The renderers below reproduce the pre-dedupe inline SQL exactly. The account
 * read/version anchor and the lead owner/eligible-owner probe each own one read
 * token here; callers pass their own projections, joins, predicates and binds
 * verbatim. Locks, command-receipt reads, native command kinds and the r8-r28
 * families are untouched.
 */
import {crmSelectBodySql} from './read-tail-r46-sql';
import {crmLeadSelectSql} from './read-tail-r51-sql';

/** Shared crm_accounts anchor. The read is the owner select-body plan over the
 * caller's projection and tail, so this module keeps no extra read token for
 * the source witness, policy read, version guard, image read and contact probe.
 * Callers pass their own tail and the optional EXISTS wrapper. */
const crmAccountReadAnchor = (projection: string, tail: string, exists = false) => {
  const read = crmSelectBodySql(`${projection}${tail}`);
  return exists ? `EXISTS(${read})` : read;
};

/** attachment-purchases.ts account source witness. */
export const crmAccountAttachmentSourceSql = (rows: string, scope: string) => crmAccountReadAnchor(
  `jsonb_build_object('account',to_jsonb(account)-'version'-'updated_at','native',native.witness)::text`,
  `\n FROM crm_accounts account JOIN (${rows}) native ON native.id=account.id AND native.company_id=account.company_id\n WHERE account.id=? AND account.company_id=? AND ${scope}`,
);

/** attachments-policy-facts.ts account version read. */
export const crmAccountPolicyReadSql = (scope: string) => crmAccountReadAnchor(
  'account.company_id,account.version',
  `\n    FROM crm_accounts account\n    WHERE account.id=? AND account.company_id=?\n      AND ${scope}`,
);

/** attachments-service-facts.ts account attachment version guard. */
export const crmAccountVersionGuardSql = (scope: string) => crmAccountReadAnchor(
  '1',
  ` FROM crm_accounts account\n    WHERE account.id=? AND account.company_id=? AND account.version=? AND ${scope}`,
  true,
);

/** attachment-read.ts account image read. */
export const crmAccountImageReadSql = (contacts: string, leads: string, native: string) => crmAccountReadAnchor(
  `jsonb_build_object('account',to_jsonb(account),
  'contacts',(${contacts}),
  'leads',(${leads}),
  ${native})::text image,
  account.finance_party_id customer_id,NULL::text quote_id,NULL::text sales_id,NULL::text reservation_id`,
  ' FROM crm_accounts account WHERE account.id=? AND account.company_id=?',
);

/** sales-package-obligations.ts contact account probe. */
export const crmAccountContactProbeSql = () => crmAccountReadAnchor(
  '1',
  ' FROM crm_accounts account WHERE account.id=contact.account_id AND account.company_id=contact.company_id',
);

/** Shared eligible-owner probe. The one read token renders byte-identically at
 * the lead guard, purchase eligibility check and work-filter probe. */
const crmLeadOwnerProbe = (join: string, predicates: string, eligible: string) =>
  crmLeadSelectSql(`l${join} WHERE ${predicates} AND ${eligible}`);

/** leads.ts owner version guard. */
export const crmLeadOwnerGuardSql = (scope: string, eligible: string) =>
  `EXISTS(${crmLeadOwnerProbe('', `l.id=? AND l.version=? AND l.owner_id=? AND ${scope}`, eligible)})`;

/** purchases.ts eligible-owner existence check. */
export const crmLeadEligibleOwnerSql = (eligible: string) =>
  crmLeadOwnerProbe('', 'l.id=? AND l.company_id=?', eligible);

/** record-purchases.ts work-filter lead owner probe. */
export const crmWorkLeadOwnerProbeSql = (eligible: string) =>
  crmLeadOwnerProbe(
    ' LEFT JOIN crm_opportunities opportunity ON opportunity.lead_id=l.id',
    "l.id=crm_work.target_id AND l.company_id=crm_work.company_id AND l.owner_id=? AND l.status<>'disqualified' AND COALESCE(opportunity.stage,'discovery') NOT IN ('won','lost')",
    eligible,
  );
