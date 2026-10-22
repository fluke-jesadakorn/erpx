import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {workerStartupProductEntry,type ProjectWorkerProductScope} from '@erpx/server-people_payroll/adapters/project-worker-startup-entry';
export type LearningWorkerProductScope=ProjectWorkerProductScope;
export const learningWorkerStartupProductEntry=(database:SqlDatabase,scope:LearningWorkerProductScope,target:{personId?:string}={})=>workerStartupProductEntry(database,scope,target,'learn');
