import 'server-only';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {projectMemberRows} from '@erpx/server-projects/adapters/work-ui-staff-facts';
export function projectMemberIds(db:SqlDatabase,projectId:string){return projectMemberRows(db,projectId);}
