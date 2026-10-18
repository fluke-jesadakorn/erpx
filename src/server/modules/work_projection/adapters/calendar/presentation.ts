import 'server-only';
import {createHash} from 'node:crypto';
import type {WorkContext} from '../../../identity_access/public/authorization';
/** The client receives an opaque partition key, never private IAM snapshots. */
export function calendarPresentationContext(actor:WorkContext){return createHash('sha256').update(JSON.stringify([actor.userId,actor.assignmentId,actor.companyId,actor.role,actor.authority,actor.iam])).digest('hex');}
