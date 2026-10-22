import "server-only";
/** Only actual Hotel document/posting identities reach the Finance reader. */
export const pmsFinanceRecordSources = {
  invoice: "SELECT p.company_id,d.finance_id id FROM pms_documents d JOIN pms_properties p ON p.id=d.property_id WHERE d.kind='invoice' AND d.finance_id IS NOT NULL",
  journal: "SELECT p.company_id,d.journal_id id FROM pms_postings d JOIN pms_properties p ON p.id=d.property_id WHERE d.journal_id IS NOT NULL UNION SELECT company_id,opening_journal_id FROM pms_import_batches WHERE state='applied' AND opening_journal_id IS NOT NULL",
  payment: "SELECT p.company_id,d.finance_id id FROM pms_documents d JOIN pms_properties p ON p.id=d.property_id WHERE d.kind IN ('receipt','refund') AND d.finance_id IS NOT NULL UNION SELECT company_id,finance_payment_id FROM pms_transfer_requests WHERE finance_payment_id IS NOT NULL UNION SELECT p.company_id,b.finance_payment_id FROM pms_import_balances b JOIN pms_properties p ON p.id=b.property_id WHERE b.finance_payment_id IS NOT NULL",
} as const;

/** Query-free source leases for retained Finance document identities. */
export function pmsFinanceRecordSourceReadLocks(database:import("@erpx/server-runtime/public/runtime/types").SqlDatabase){return database.prepare('LOCK TABLE pms_properties,pms_documents,pms_postings,pms_import_batches,pms_transfer_requests,pms_import_balances IN SHARE ROW EXCLUSIVE MODE');}

/** Full retained native relationship headers remain server-only. */
export const pmsFinanceRecordSourceImages={
 journal:`SELECT p.company_id,d.journal_id id,jsonb_build_object('property',to_jsonb(p),'posting',to_jsonb(d)) source_image FROM pms_postings d JOIN pms_properties p ON p.id=d.property_id WHERE d.journal_id IS NOT NULL UNION ALL SELECT company_id,opening_journal_id,to_jsonb(d) FROM pms_import_batches d WHERE state='applied' AND opening_journal_id IS NOT NULL`,
 payment:`SELECT p.company_id,d.finance_id id,jsonb_build_object('property',to_jsonb(p),'document',to_jsonb(d)) source_image FROM pms_documents d JOIN pms_properties p ON p.id=d.property_id WHERE d.kind IN ('receipt','refund') AND d.finance_id IS NOT NULL UNION ALL SELECT company_id,finance_payment_id,to_jsonb(d) FROM pms_transfer_requests d WHERE finance_payment_id IS NOT NULL UNION ALL SELECT p.company_id,b.finance_payment_id,jsonb_build_object('property',to_jsonb(p),'balance',to_jsonb(b)) FROM pms_import_balances b JOIN pms_properties p ON p.id=b.property_id WHERE b.finance_payment_id IS NOT NULL`,
} as const;
