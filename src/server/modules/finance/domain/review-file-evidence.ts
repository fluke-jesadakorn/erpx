import type { FinanceError } from './invoice-line-revision';
import type { FinanceReviewFileSource } from '@/server/modules/files/public/finance-review-contract';
export type FinanceReviewActor = {
    companyId: string;
    userId: string;
};
export type FinanceReviewManifest = Array<{
    linkId: string;
    sourceId: string | null;
    purposeKey: string;
    checksumSha256: string | null;
    mediaType: string | null;
    bytes: number | null;
}>;
const targets: Record<string, string> = { 'finance.billing-correction.post': 'finance_billing_correction', 'finance.business-zero.approve': 'finance_business_bill', 'finance.plan.approve': 'finance_plan_edition', 'finance.book.approve': 'finance_book_rule', 'finance.book-adjustment.post': 'finance_book_adjustment', 'finance.bank-group.confirm': 'finance_bank_match_group', 'finance.collection.approve': 'finance_collection_case', 'finance.business-contract.approve': 'finance_business_contract', 'finance.recognition.post': 'finance_recognition_run', 'finance.instrument.approve': 'finance_instrument', 'finance.instrument-run.post': 'finance_instrument_run', 'finance.asset-policy.approve': 'finance_asset_policy', 'finance.asset-policy-run.post': 'finance_asset_policy_run', 'finance.group-edition.approve': 'finance_group_report', 'finance.ownership.approve': 'finance_ownership_interest', 'finance.acquisition.approve': 'finance_acquisition', 'finance.intercompany-pair.confirm': 'finance_intercompany_pair' };
export function createFinanceReviewFilePolicy(error: FinanceError) {
    function reviewedTarget(operation: string, version: number, input: unknown) { const target = targets[operation]; if (!target)
        return null; const expected = typeof input === 'object' && input !== null ? Number((input as Record<string, unknown>).expectedVersion) : NaN; if (!Number.isSafeInteger(expected) || expected < 1 || version !== expected + 1)
        throw error(409, 'FINANCE_REVIEW_VERSION_REQUIRED', 'Retain the exact Finance source version reviewed by this command.'); return { target, expected }; }
    function manifest(files: FinanceReviewFileSource[]): FinanceReviewManifest { if (files.some(f => f.link_state !== 'bound' || f.set_state !== 'bound' || f.source_state !== 'available' || !f.source_id || !f.checksum_sha256))
        throw error(409, 'FINANCE_FILE_EVIDENCE_NOT_READY', 'Every retained Finance file must be clean and bound to the current record before independent review. Finish scanning, bind or withdraw unfinished files.'); return files.map(f => ({ linkId: f.link_id, sourceId: f.source_id, purposeKey: f.purpose_key, checksumSha256: f.checksum_sha256, mediaType: f.media_type, bytes: f.bytes })); }
    return { reviewedTarget, manifest };
}
