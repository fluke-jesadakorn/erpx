import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
export function createApprovalsSourceRepository(database:SqlDatabase){return {readExpenseDecisionEvidenceAssignmentsreceiptsperson2(record_id:string){return database.prepare(
        "SELECT id,revision,checksum_sha256,original_name FROM attachments WHERE owner_type='expense_claim' AND owner_id=? AND revoked_at IS NULL ORDER BY id",
      )
      .bind(record_id)
      .all();}};}
