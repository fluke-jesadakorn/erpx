import "server-only";
import { holdingResources } from "@/server/modules/runtime/public/runtime";
import { handbookList } from "@/server/modules/runtime/public/handbook";
import { handbookEditorOptions } from "@/server/modules/runtime/public/handbook";
import { handbookAccess } from "@/server/modules/runtime/public/handbook";
import { handbookArticle } from "@/server/modules/runtime/public/handbook";
import { inboxSummary } from "@/server/modules/collaboration/public/inbox";
import {messagingOverview,messagingHistory} from '../public/messaging';
import {createHash} from 'node:crypto';
import type {WorkContext} from '../../identity_access/public/authorization';

type ReadArguments<F extends (...args: never[]) => unknown> = Parameters<F> extends [unknown, ...infer Rest] ? Rest : never;

/** Native composition for presentation readers; the supplied Holding connection stays private. */
export async function collaborationPresentationQueries(holdingId: string) {
  const { write } = await holdingResources(holdingId);
  return {
    handbookList: (...args: ReadArguments<typeof handbookList>) => handbookList(write, ...args),
    handbookEditorOptions: (...args: ReadArguments<typeof handbookEditorOptions>) => handbookEditorOptions(write, ...args),
    handbookAccess: (...args: ReadArguments<typeof handbookAccess>) => handbookAccess(write, ...args),
    handbookArticle: (...args: ReadArguments<typeof handbookArticle>) => handbookArticle(write, ...args),
    inboxSummary: (...args: ReadArguments<typeof inboxSummary>) => inboxSummary(write, ...args),
    messagingWorkspace:async(actor:WorkContext,roomId?:string)=>{
      const overview=await messagingOverview(write,actor),history=roomId&&overview.rooms.some(room=>room.id===roomId)?await messagingHistory(write,actor,roomId):undefined;
      const context=createHash('sha256').update(JSON.stringify([actor.userId,actor.companyId,actor.branchId,actor.departmentId,actor.assignmentId,actor.role,actor.authority??null,actor.iam??null])).digest('hex');
      return {overview,history,context};
    },
  };
}
