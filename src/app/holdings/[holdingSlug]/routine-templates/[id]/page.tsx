import { staffPresentationQueries } from "@/server/modules/work_projection/public/presentation-context-queries";

import { FeaturePage, FeatureTabs,FeatureWidget,WidgetGrid } from "@/components/features/feature-page";
import {
RoutineScheduleControls,
TaskEditor,
} from "@/components/modules/staff-work-controls";
import { pageError } from "@/server/modules/work_projection/public/page-error";


import { RecordLink as Link } from "@/components/records/record-link";
export default async function Routine({
  params,
}: {
  params: Promise<{ holdingSlug: string; id: string }>;
}) {
  const { holdingSlug, id } = await params,
    { actor, queries, base, options, projects } = await staffPresentationQueries(holdingSlug),
    template = await queries.getTemplate(actor, id).catch((e) => {
      return pageError(e, holdingSlug);
    }),
    canEdit = await queries.canEditTemplate(actor, template),
    schedules = (
      await queries.routineTemplateSchedules(id, actor.companyId)
    ).results;
  return (
    <FeaturePage className="ui-page-content" data-source="live">
      <Link href={`${base}/routine-templates`}>← Routine templates</Link>
      <h1>{template.title}</h1><FeatureTabs/><WidgetGrid pageKey="routine-templates-detail">
      <p>{template.description}</p>
      <p>
        Template version {template.version} ·{" "}
        {template.published ? "Published" : "Draft"}. Edits affect future
        occurrences.
      </p>
      <FeatureWidget appearance="standard" id="routine-template" title="Routine template">{canEdit && (
        <TaskEditor appearance="standard"
          key={template.version}
          base={base}
          options={options}
          projects={projects}
          userId={actor.userId}
          template={template}
        />
      )}
      <TaskEditor appearance="standard"
        key={`clone-${template.version}`}
        base={base}
        options={options}
        projects={projects}
        userId={actor.userId}
        template={template}
        clone
      />
      </FeatureWidget><FeatureWidget appearance="standard" id="routine-schedule" title="Schedule"><RoutineScheduleControls appearance="standard"
        base={base}
        template={template}
        schedules={schedules}
        canEdit={canEdit}
      />
    </FeatureWidget></WidgetGrid></FeaturePage>
  );
}
