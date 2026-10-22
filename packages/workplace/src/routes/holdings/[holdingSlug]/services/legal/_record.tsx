import {requireWorkContext,publicScope} from '@erpx/server-identity_access/public/authorization';
import {workspaceBase} from '@erpx/feature-work_projection/lib/workspace';
import {legalPresentationQueries} from '@erpx/server-legal/public/presentation-queries';
import {LegalWorkspace} from '@erpx/feature-records/components/loaders/legal--legal-workspace';
export default async function LegalRecord({params}:{params:Promise<{holdingSlug:string;id:string}>}){
 const {holdingSlug,id}=await params,actor=await requireWorkContext(holdingSlug),presentation=await legalPresentationQueries(actor.holdingId);
 const [initial,initialDetail]=await Promise.all([presentation.recordData(actor),presentation.detail(actor,id)]);
 return <LegalWorkspace base={workspaceBase(publicScope(actor))} initial={initial} initialDetail={initialDetail} initialKind={initialDetail.item.kind}/>;
}
