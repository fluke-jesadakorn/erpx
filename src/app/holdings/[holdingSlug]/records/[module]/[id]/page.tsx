import Link from "next/link";
import { notFound } from "next/navigation";
import { ArrowLeft } from "lucide-react";
import { findModule,moduleHref } from "@/data/module-registry";
import { publicScope,requireWorkContext } from "@/server/authorization";
import { recordContext } from "@/server/records";
import { workspaceBase } from "@/lib/workspace";
import { RecordWorkbench,type RecordContext } from "@/components/modules/record-workbench";
export const dynamic="force-dynamic";
export default async function RecordPage({params}:PageProps<"/holdings/[holdingSlug]/records/[module]/[id]">){const {holdingSlug,module,id}=await params;const definition=findModule(module);if(!definition)notFound();const actor=await requireWorkContext(holdingSlug);let data;try{data=await recordContext(actor,module,id);}catch{notFound();}const base=workspaceBase(publicScope(actor));const section=definition.sections.find(item=>item.id===data.record.sectionId);return <main className="content" data-source="live"><Link className="back-link" href={moduleHref(base,definition,data.record.sectionId)}><ArrowLeft size={14}/>Back to {definition.name.toLowerCase()}</Link><RecordWorkbench initial={data as RecordContext} workspaceBase={base} moduleName={definition.name} sectionLabel={section?.label??data.record.sectionId} role={actor.role}/></main>}
