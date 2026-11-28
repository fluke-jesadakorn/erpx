import "server-only";
import {publicScope,type WorkContext} from "@erpx/identity-access-server/public/authorization";
import {workspaceBase} from "@erpx/work-projection-ui/lib/workspace";
import type {SqlDatabase} from "@erpx/platform-runtime-server/public/runtime/types";
import {frontUnreadHomeCount} from "@erpx/collaboration-server/public/work-ui-home-counts";
import {createNavigationCountService} from "@erpx/work-projection-server/application/navigation-counts";
import {NativeHomeReadCursorError,nativeMarkHomeRead} from "./native-home-read-cursor-command";
import {ApiError} from "@erpx/platform-runtime-server/public/http";

function bind(db:SqlDatabase){return createNavigationCountService({
  now:Date.now,
  base:actor=>workspaceBase(publicScope(actor)),
  // Native axum command owns the home_read_cursors write in the caller-bound
  // Holding unit. No SQL and no fallback remain for this write.
  mark:async(actor,base,now)=>{
    const key=`home-read:${actor.holdingId}:${actor.companyId}:${actor.userId}:${base}:${now}`;
    try{await nativeMarkHomeRead(actor,base,now,key);}
    catch(error){if(error instanceof NativeHomeReadCursorError)throw new ApiError(error.status??503,"NATIVE_WORK_UNAVAILABLE","The native Work service is unavailable.");throw error;}
  },
  unread:(actor,base,now)=>frontUnreadHomeCount(db,actor,base,now),
});}
export function markHomeRead(db:SqlDatabase,actor:WorkContext,now=Date.now()){return bind(db).markHomeRead(actor,now);}
export function unreadHomeCount(db:SqlDatabase,actor:WorkContext,now=Date.now()){return bind(db).unreadHomeCount(actor,now);}
