import {PosStartup} from '@erpx/feature-pos/components/pos-startup';
import {requireWorkContext,publicScope} from '@erpx/server-identity_access/public/authorization';
import {posStartupPrerequisites} from '@erpx/server-finance/public/pos-startup-prerequisites';
import {posPostingSetup} from '@erpx/server-commerce/public/pos-posting-setup';
import {posStartupWorkspace} from '@erpx/server-pos/public/startup';
import {workspaceBase} from '@erpx/feature-work_projection/lib/workspace';
import {pageError} from '@erpx/server-work_projection/public/page-error';
export const dynamic='force-dynamic';
export default async function Page({params}:{params:Promise<{holdingSlug:string}>}){
 const {holdingSlug}=await params,actor=await requireWorkContext(holdingSlug),[prerequisites,mapping,workspace]=await Promise.all([posStartupPrerequisites(actor),posPostingSetup(actor),posStartupWorkspace(actor)]).catch(error=>pageError(error,holdingSlug));
 return <PosStartup base={workspaceBase(publicScope(actor))} initial={{prerequisites,mapping,workspace}}/>;
}
