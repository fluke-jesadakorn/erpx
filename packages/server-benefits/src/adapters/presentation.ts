import 'server-only';
import {createHash} from 'node:crypto';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
/** Opaque draft identity only; commands still resolve original current authority. */
export function benefitPresentationContext(actor:WorkContext){return createHash('sha256').update(JSON.stringify([actor.holdingId,actor.companyId,actor.branchId,actor.departmentId,actor.userId,actor.assignmentId,actor.role,actor.authority??null,actor.iam?.identityVersion??null,actor.iam?.roleRevision??null,actor.iam?.hrAppointmentVersion??null])).digest('hex');}
