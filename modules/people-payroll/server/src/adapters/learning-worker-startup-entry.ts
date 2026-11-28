import type {SqlDatabase} from '@erpx/platform-runtime-server/public/runtime/types';
import {workerStartupProductEntry,type ProjectWorkerProductScope} from '@erpx/people-payroll-server/adapters/project-worker-startup-entry';
export type LearningWorkerProductScope=ProjectWorkerProductScope;
export const learningWorkerStartupProductEntry=(database:SqlDatabase,scope:LearningWorkerProductScope,target:{personId?:string}={})=>workerStartupProductEntry(database,scope,target,'learn');
