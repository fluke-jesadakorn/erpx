import 'server-only';
import {nativeWorkflowWrite} from './native-workflow-write';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
export function stageBookAdjustmentDecision(db:SqlDatabase,facts:{evidenceId:string;key:string;id:string;op:string;userId:string;assignmentId:string;expectedVersion:number;inputHash:string;evidenceAt:number}){const {evidenceId,key,id,op,userId,assignmentId,expectedVersion,inputHash,evidenceAt}=facts;return db
          nativeWorkflowWrite('wf_native_finance_book_adjustment_effects_4', [evidenceId, key, id, op, userId, assignmentId, expectedVersion, inputHash, evidenceAt,]);}
