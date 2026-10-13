import "server-only";
import {publicScope,type WorkContext} from "../../identity_access/public/authorization";
import {workspaceBase} from "@/lib/work_projection/workspace";
import type {SqlDatabase} from "../../runtime/public/runtime/types";
import {frontUnreadHomeCount} from "../../collaboration/public/work-ui-home-counts";
import {createNavigationCountService} from "../application/navigation-counts";
async function mark(db:SqlDatabase,actor:WorkContext,base:string,now:number) {
  await db.prepare(`INSERT INTO home_read_cursors(company_id,user_id,workspace_base,read_through)
    VALUES(?,?,?,?) ON CONFLICT(company_id,user_id,workspace_base)
    DO UPDATE SET read_through=GREATEST(home_read_cursors.read_through,excluded.read_through)`)
    .bind(actor.companyId, actor.userId, base, now).run();
}

function bind(db:SqlDatabase){return createNavigationCountService({now:Date.now,base:actor=>workspaceBase(publicScope(actor)),mark:(actor,base,now)=>mark(db,actor,base,now),unread:(actor,base,now)=>frontUnreadHomeCount(db,actor,base,now)});}
export function markHomeRead(db:SqlDatabase,actor:WorkContext,now=Date.now()){return bind(db).markHomeRead(actor,now);}
export function unreadHomeCount(db:SqlDatabase,actor:WorkContext,now=Date.now()){return bind(db).unreadHomeCount(actor,now);}
