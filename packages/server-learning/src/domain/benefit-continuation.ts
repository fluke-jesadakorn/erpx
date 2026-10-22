import type {LearningRow} from '@erpx/server-learning/domain/learning';
import type {LearningPurchaseControls} from '@erpx/server-learning/domain/purchase-controls';
/** Existing owner detail only; no route may submit this as a command. */
export type LearningBenefitSource={assignment:LearningRow;handoff:LearningRow|null;certificate:LearningRow|null;purchases?:LearningPurchaseControls};
export type LearningBenefitContinuation={eligibilityCode:string};
