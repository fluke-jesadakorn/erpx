import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {nativeWorkflowReadFirst} from './native-workflow-read';
export function leaveEvidenceDecisions(database:SqlDatabase){return {recordedDecision(companyId:string,leaveRequestId:string,userId:string){return nativeWorkflowReadFirst<Record<string,unknown>>('wf_native_leave_evidence_decision',[companyId,leaveRequestId,userId],companyId,database);}};}
