/** Declared isolated Quality evidence source for cross-owner Files oracles.
 *
 * Inspection, release and controlled disposition are owned by the native
 * Quality HTTP commands. This fixture writes the same hold/inspection/
 * disposition source rows those commands commit, applies the real shared Files
 * binding preparation and passes the shared workflow command boundary so the
 * Files suite retains its receipt/audit/outbox assertions. It is only a test
 * oracle; production Quality writes remain native-only.
 */
import {isTransactionalSqlDatabase,type SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import {prepareBusinessEvidence} from '@erpx/server-records/public/business-evidence';
import {executeCommand} from '@erpx/server-workflow/public/commands';

export type QualityEvidenceSeed = {
  companyId: string;
  holdId: string;
  actor: WorkContext;
  setId: string;
  command: string;
  purposeKey: string;
  componentRef: string;
  kind: 'inspection' | 'disposition';
  expectedVersion: number;
  reason: string;
  createdAt?: number;
};

export type QualityEvidenceSeedReceipt = {
  id: string;
  status: 'awaiting_release' | 'disposition_pending';
  version: number;
  countId?: string;
};

export async function seedNativeQualityEvidence(
  db: SqlDatabase,
  input: QualityEvidenceSeed,
): Promise<QualityEvidenceSeedReceipt> {
  if (!isTransactionalSqlDatabase(db))
    throw new Error('NATIVE_QUALITY_EVIDENCE_SEED_REQUIRES_TRANSACTION');
  const now = input.createdAt ?? Date.now();
  const inspection = input.kind === 'inspection';
  const inspectionId = `inspection-${input.command}`;
  const countId = `quality-scrap-${input.command}`;
  const status = inspection ? 'awaiting_release' : 'disposition_pending';
  return db.transaction(async tx => {
    const receipt = await tx.prepare('SELECT result_json FROM command_receipts WHERE command_id=?').bind(input.command).first<{result_json:string|null}>();
    if (receipt) return JSON.parse(receipt.result_json ?? '{}') as QualityEvidenceSeedReceipt;
    const hold = await tx.prepare(
      'SELECT id,version,branch_id,warehouse_id FROM inventory_quality_holds WHERE id=? AND company_id=? FOR UPDATE',
    ).bind(input.holdId, input.companyId).first<{id:string;version:number;branch_id:string|null;warehouse_id:string}>();
    if (!hold) throw new Error('NATIVE_QUALITY_EVIDENCE_HOLD_NOT_FOUND');
    if (Number(hold.version)!==input.expectedVersion) throw new Error('NATIVE_QUALITY_EVIDENCE_VERSION_CHANGED');
    const prepared = await prepareBusinessEvidence(tx, input.actor, {
      attachmentSetIds:[input.setId],
      targetTypes:['quality_hold','inventory_quality_hold'],
      targetId:input.holdId,
      expectedTargetVersion:input.expectedVersion,
      commandId:input.command,
      bindingType:inspection?'quality_inspection':'quality_disposition',
      bindingId:inspection?inspectionId:countId,
      contexts:[{purposeKey:input.purposeKey,componentKind:'action',componentRef:input.componentRef}],
    });
    if (!prepared) throw new Error('NATIVE_QUALITY_EVIDENCE_BINDING_REQUIRED');
    const effects=[...prepared.guards];
    if (inspection) {
      effects.push(tx.prepare(
        'INSERT INTO inventory_quality_inspections(id,hold_id,reviewed_version,result,evidence,inspected_by,created_at) VALUES(?,?,?,?,?,?,?)',
      ).bind(inspectionId,input.holdId,input.expectedVersion,'pass',input.reason,input.actor.userId,now));
      effects.push(tx.prepare(
        "UPDATE inventory_quality_holds SET status='awaiting_release',last_result='pass',last_inspector=?,version=version+1,updated_at=? WHERE id=?",
      ).bind(input.actor.userId,now,input.holdId));
    } else {
      effects.push(tx.prepare(
        "INSERT INTO inventory_counts(id,company_id,branch_id,warehouse_id,number,count_date,reason,status,version,net_value_minor,created_by,prepared_by,created_at,updated_at,quality_hold_id) VALUES(?,?,?,?,?,'2026-09-12',?,'submitted',1,-1000,?,?,?,?,?)",
      ).bind(countId,input.companyId,hold.branch_id,hold.warehouse_id,`QSCRAP-${input.command.slice(0,24)}`,input.reason,input.actor.userId,input.actor.userId,now,now,input.holdId));
      effects.push(tx.prepare(
        "INSERT INTO inventory_quality_disposition_events(id,hold_id,count_id,action,actor_id,reviewed_version,reason,created_at) VALUES(?,?,?,'request',?,?,?,?)",
      ).bind(`quality-disposition-${input.command}`,input.holdId,countId,input.actor.userId,input.expectedVersion,input.reason,now));
      effects.push(tx.prepare(
        "UPDATE inventory_quality_holds SET status='disposition_pending',disposition_kind='scrap',disposition_reason=?,disposition_requested_by=?,disposition_count_id=?,disposition_quantity_milli=1000,version=version+1,updated_at=? WHERE id=?",
      ).bind(input.reason,input.actor.userId,countId,now,input.holdId));
    }
    effects.push(...prepared.steps);
    const result: QualityEvidenceSeedReceipt = {id:input.holdId,status,version:input.expectedVersion+1,...(inspection?{}:{countId})};
    return executeCommand(tx,input.actor,input.command,inspection?'quality.hold.inspect':'quality.hold.disposition.scrap',{expectedVersion:input.expectedVersion,setId:input.setId},input.holdId,input.expectedVersion+1,result,effects,{companyId:input.actor.companyId,branchId:hold.branch_id,departmentId:null});
  });
}
