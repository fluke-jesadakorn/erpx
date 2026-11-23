import type {Clock} from '@erpx/server-shared/index';
import type {ReceiptQualityHold} from '@erpx/server-quality/domain/quality';
export interface QualityRepository<Effects>{
  receiptHold(input:ReceiptQualityHold,now:number):Promise<Effects>;
}
/** The retained Quality work/detail_core projection is owned by the native
 * `quality_work.rs` port and mounted over the WorkContext-header read router.
 * Only protective receiving/return effects stay in this TypeScript application
 * until their own owner packet retires them. */
export function createQualityApplication<Effects>(ports:{clock:Clock;repository():Promise<QualityRepository<Effects>>}){
  async function receiptQualityHoldSteps(input:ReceiptQualityHold){const r=await ports.repository(),now=ports.clock.now();return r.receiptHold(input,now);}
  return {receiptQualityHoldSteps};
}
