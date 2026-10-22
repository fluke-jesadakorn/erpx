import {RoutineTemplateEditor} from '@erpx/feature-operations/components/routine-template-editor';
import {staffPickerDisplayOptions} from '@erpx/feature-work_projection/lib/staff-picker-display';
import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import { staffPresentationQueries } from "@erpx/server-work_projection/public/presentation-context-queries";

import { FeaturePage, FeatureTabs,FeatureWidget,WidgetGrid } from "@erpx/feature-shared/components/features/feature-page";
import { RoutineScheduleControls } from "@erpx/feature-records/components/loaders/modules--staff-work-controls";
import { pageError } from "@erpx/server-work_projection/public/page-error";


import { RecordLink as Link } from "@erpx/feature-records/components/record-link";
export default async function Routine({
  params,
}: {
  params: Promise<{ holdingSlug: string; id: string }>;
}) {
  const { holdingSlug, id } = await params,
    { actor, queries, base, options, projects,staff } = await staffPresentationQueries(holdingSlug,{routineProjects:true}),
    template = await queries.getTemplate(actor, id).catch((e) => {
      return pageError(e, holdingSlug);
    }),
    purchases=await queries.routinePurchases(),
    canEdit = await queries.canEditTemplate(actor, template),
    schedules = (
      await queries.routineTemplateSchedules(id, actor.companyId)
    ).results;
  return (
    <FeaturePage className="ui-page-content" data-source="live">
      <Link href={canonicalUiHref(`${base}/routine-templates`)}>← Routine templates</Link>
      <h1>{template.title}</h1><FeatureTabs/><WidgetGrid pageKey="routine-templates-detail">
      <p>{template.description}</p>
      <p>
        Template version {template.version} ·{" "}
        {template.published ? "Published" : "Draft"}. Edits affect future
        occurrences.
      </p>
      <FeatureWidget appearance="standard" id="routine-template" title="Routine template">{canEdit && (
        <RoutineTemplateEditor appearance="standard"
          key={template.version}
          base={base}
          options={staffPickerDisplayOptions(options)}
          projects={projects.map(({id,title})=>({id,title}))}
          userId={actor.userId}
          template={template}
          purchases={purchases}
          canWrite={!!staff}
        />
      )}
      <RoutineTemplateEditor appearance="standard"
        key={`clone-${template.version}`}
        base={base}
        options={staffPickerDisplayOptions(options)}
        projects={projects.map(({id,title})=>({id,title}))}
        userId={actor.userId}
        template={template}
        purchases={purchases}
          canWrite={!!staff}
        clone
      />
      </FeatureWidget><FeatureWidget appearance="standard" id="routine-schedule" title="Schedule"><RoutineScheduleControls appearance="standard"
        base={base}
        template={template}
        schedules={schedules}
        canEdit={canEdit}
        canRun={!!staff}
        purchases={purchases}
      />
    </FeatureWidget></WidgetGrid></FeaturePage>
  );
}
