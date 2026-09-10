import Link from "next/link";
import { notFound } from "next/navigation";
import { ArrowLeft } from "lucide-react";
import { findModule,moduleHref } from "@/data/module-registry";
import { getModuleSamples } from "@/data/preview/samples";
import { publicScope,requireWorkContext } from "@/server/authorization";
import { workspaceBase } from "@/lib/workspace";
import { CreateRecordForm } from "@/components/modules/create-record-form";
export default async function NewRecordPage({params,searchParams}:PageProps<"/holdings/[holdingSlug]/records/[module]/new">){const {holdingSlug,module}=await params;const search=await searchParams;const definition=findModule(module);const section=definition?.sections.find(item=>item.id===search.section);if(!definition||!section)notFound();const actor=await requireWorkContext(holdingSlug);if(!actor.modules.includes(module))notFound();const base=workspaceBase(publicScope(actor));const samples=getModuleSamples(module,section.id);if(!samples)notFound();return <main className="content"><Link className="back-link" href={moduleHref(base,definition,section.id)}><ArrowLeft size={14}/>Back to {definition.name.toLowerCase()}</Link><header className="page-head"><div><span className="eyebrow">{definition.name}</span><h1>Create {section.label.toLowerCase()} record</h1><p className="lede">All documented fields are editable. Save as a Draft, then move it through the shared review workflow.</p></div></header><CreateRecordForm workspaceBase={base} moduleId={module} sectionId={section.id} initialFields={samples.fields}/></main>}
