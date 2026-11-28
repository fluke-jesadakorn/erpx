import {ppRd37CoalesceLegalName} from './pp-read-tail-r37-sources';
/** Fixed journal aliases preserve Company membership and unlinked-account fallback in one snapshot. */
export function financeJournalPeopleFacts(){return {
 createdBy:ppRd37CoalesceLegalName("created_by", "created_by_name"),
 postedBy:ppRd37CoalesceLegalName("posted_by", "posted_by_name"),
} as const;}
