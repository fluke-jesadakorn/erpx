import 'server-only';
import type {SqlDatabase} from '@erpx/platform-runtime-server/public/runtime/types';
import {projectMemberRows} from '@erpx/projects-server/adapters/work-ui-staff-facts';
export function projectMemberIds(db:SqlDatabase,projectId:string){return projectMemberRows(db,projectId);}
