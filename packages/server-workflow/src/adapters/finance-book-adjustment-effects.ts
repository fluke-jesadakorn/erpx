import 'server-only';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
export function stageBookAdjustmentDecision(db:SqlDatabase,facts:{evidenceId:string;key:string;id:string;op:string;userId:string;assignmentId:string;expectedVersion:number;inputHash:string;evidenceAt:number}){const {evidenceId,key,id,op,userId,assignmentId,expectedVersion,inputHash,evidenceAt}=facts;return db
          .prepare("INSERT INTO decision_evidence VALUES(?,?,?,?,?,?,?,?,?)")
          .bind(
            evidenceId,
            key,
            id,
            op,
            userId,
            assignmentId,
            expectedVersion,
            inputHash,
            evidenceAt,
          );}
