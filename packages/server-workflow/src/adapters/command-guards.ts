import type {SqlDatabase,SqlStatement} from '@erpx/server-runtime/public/runtime/types';
import {ApiError} from '@erpx/server-runtime/public/http';
import {createWorkflowCommandGuards} from '@erpx/server-identity_access/public/workflow-command-authority';
import {assertion} from '@erpx/server-workflow/public/command-assertions';
import {financialConstraintError as domainFinancialConstraintError,type CommandActor} from '@erpx/server-workflow/domain/commands';
const {workflowWorkContextGuards}=createWorkflowCommandGuards(assertion);

export function workContextGuards(db:SqlDatabase,actor:CommandActor,id:string):SqlStatement[]{return workflowWorkContextGuards(db,actor,id);}
export function financialConstraintError(error:unknown){const failure=domainFinancialConstraintError(error);return failure?new ApiError(failure.status,failure.code,failure.message):null;}
