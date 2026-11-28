import {PosStartup} from '@erpx/pos-ui/components/pos-startup';
import {requireWorkContext,publicScope} from '@erpx/identity-access-server/public/authorization';
import {posStartupPrerequisites} from '@erpx/finance-server/public/pos-startup-prerequisites';
import {posPostingSetup} from '@erpx/commerce-server/public/pos-posting-setup';
import {posStartupWorkspace} from '@erpx/pos-server/public/startup';
import {workspaceBase} from '@erpx/work-projection-ui/lib/workspace';
import {pageError} from '@erpx/work-projection-server/public/page-error';
export const dynamic='force-dynamic';
export default async function Page({params}:{params:Promise<{holdingSlug:string}>}){
 const {holdingSlug}=await params,actor=await requireWorkContext(holdingSlug),[prerequisites,mapping,workspace]=await Promise.all([posStartupPrerequisites(actor),posPostingSetup(actor),posStartupWorkspace(actor)]).catch(error=>pageError(error,holdingSlug));
 return <PosStartup base={workspaceBase(publicScope(actor))} initial={{prerequisites,mapping,workspace}}/>;
}
