import type {SqlDatabase,SqlStatement} from '@erpx/platform-runtime-server/public/runtime/types';
import {ApiError} from '@erpx/platform-runtime-server/public/http';
import {createWorkflowCommandGuards} from '@erpx/identity-access-server/public/workflow-command-authority';
import {assertion} from '@erpx/workflow-server/public/command-assertions';
import {financialConstraintError as domainFinancialConstraintError,type CommandActor} from '@erpx/workflow-server/domain/commands';
const {workflowWorkContextGuards}=createWorkflowCommandGuards(assertion);

export function workContextGuards(db:SqlDatabase,actor:CommandActor,id:string):SqlStatement[]{return workflowWorkContextGuards(db,actor,id);}
export function financialConstraintError(error:unknown){const failure=domainFinancialConstraintError(error);return failure?new ApiError(failure.status,failure.code,failure.message):null;}
