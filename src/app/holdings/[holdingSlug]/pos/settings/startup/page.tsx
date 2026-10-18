import {PosStartup} from '@/components/pos/pos-startup';
import {requireWorkContext,publicScope} from '@/server/modules/identity_access/public/authorization';
import {posStartupPrerequisites} from '@/server/modules/finance/public/pos-startup-prerequisites';
import {posPostingSetup} from '@/server/modules/commerce/public/pos-posting-setup';
import {posStartupWorkspace} from '@/server/modules/pos/public/startup';
import {workspaceBase} from '@/lib/work_projection/workspace';
import {pageError} from '@/server/modules/work_projection/public/page-error';
export const dynamic='force-dynamic';
export default async function Page({params}:{params:Promise<{holdingSlug:string}>}){
 const {holdingSlug}=await params,actor=await requireWorkContext(holdingSlug),[prerequisites,mapping,workspace]=await Promise.all([posStartupPrerequisites(actor),posPostingSetup(actor),posStartupWorkspace(actor)]).catch(error=>pageError(error,holdingSlug));
 return <PosStartup base={workspaceBase(publicScope(actor))} initial={{prerequisites,mapping,workspace}}/>;
}
