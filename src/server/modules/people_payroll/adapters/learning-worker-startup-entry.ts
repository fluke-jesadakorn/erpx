import type {SqlDatabase} from '../../runtime/public/runtime/types';
import {workerStartupProductEntry,type ProjectWorkerProductScope} from './project-worker-startup-entry';
export type LearningWorkerProductScope=ProjectWorkerProductScope;
export const learningWorkerStartupProductEntry=(database:SqlDatabase,scope:LearningWorkerProductScope,target:{personId?:string}={})=>workerStartupProductEntry(database,scope,target,'learn');
