import { requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { ownedProjectWbs } from "@erpx/feature-projects/public/native-project-wbs-owned";
import { pageError } from "@erpx/server-work_projection/public/page-error";
import { Card } from "@erpx/ui/components/foundation";

export default async function Page({params}:{params:Promise<{holdingSlug:string;id:string}>}) {
  const {holdingSlug,id}=await params, actor=await requireWorkContext(holdingSlug);
  const wbs=await ownedProjectWbs(actor,holdingSlug,id).catch(error=>pageError(error,holdingSlug));
  if(!wbs) return <main><h1>Project WBS unavailable</h1></main>;
  return <main><h1>{wbs.project.title}</h1><Card as="section" appearance="standard" className="ui-panel"><h2>Native WBS register</h2><p>{wbs.project.lifecycle_status} · version {wbs.project.version}</p>{wbs.milestones.map(milestone=><p key={milestone.id}>{milestone.title} · {milestone.state} · due {new Date(milestone.due_at).toISOString().slice(0,10)}{milestone.depends_on_titles.length?` · after ${milestone.depends_on_titles.join(", ")}`:""}</p>)}</Card></main>;
}
