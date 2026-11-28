import {RoutineTemplateEditor} from '@erpx/operations-ui/components/routine-template-editor';
import {
  nativeRoutineEditorContext, nativeRoutineTemplateSchedules,
} from '@erpx/operations-ui/public/native-routines-read';
import { publicScope, requireWorkContext } from "@erpx/identity-access-server/public/authorization";
import { workspaceBase } from "@erpx/work-projection-ui/lib/workspace";
import { canonicalUiHref } from "@erpx/work-projection-ui/lib/destination-registry";
import { routineReadRequest } from "../routine-read-request";

import { FeaturePage, FeatureTabs,FeatureWidget,WidgetGrid } from "@erpx/platform-shared-ui/components/features/feature-page";
import { RoutineScheduleControls } from "@erpx/records-ui/components/loaders/modules--staff-work-controls";
import { pageError } from "@erpx/work-projection-server/public/page-error";

import { RecordLink as Link } from "@erpx/records-ui/components/record-link";
async function nativeTemplate(
  holdingSlug: string,
  id: string,
  actor: Awaited<ReturnType<typeof requireWorkContext>>,
) {
  const request = await routineReadRequest(holdingSlug, `routine-templates/${encodeURIComponent(id)}/schedules`, actor);
  return nativeRoutineTemplateSchedules(request, holdingSlug, id, actor.userId);
}
async function nativeEditor(holdingSlug: string, actor: Awaited<ReturnType<typeof requireWorkContext>>) {
  const request = await routineReadRequest(holdingSlug, 'routine-template-editor-context', actor);
  return nativeRoutineEditorContext(request, holdingSlug, actor.userId);
}
export default async function Routine({
  params,
}: {
  params: Promise<{ holdingSlug: string; id: string }>;
}) {
  const { holdingSlug, id } = await params,
    actor = await requireWorkContext(holdingSlug),
    base = workspaceBase(publicScope(actor)),
    {template, canEdit, schedules} = await nativeTemplate(holdingSlug, id, actor).catch((e) => {
      return pageError(e, holdingSlug);
    }),
    {projects, options, canWrite, purchases} = await nativeEditor(holdingSlug, actor).catch((e) => {
      return pageError(e, holdingSlug);
    });
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
          options={options}
          projects={projects}
          userId={actor.userId}
          template={template}
          purchases={purchases??undefined}
          canWrite={canWrite}
        />
      )}
      <RoutineTemplateEditor appearance="standard"
        key={`clone-${template.version}`}
        base={base}
        options={options}
        projects={projects}
        userId={actor.userId}
        template={template}
        purchases={purchases??undefined}
        canWrite={canWrite}
        clone
      />
      </FeatureWidget><FeatureWidget appearance="standard" id="routine-schedule" title="Schedule"><RoutineScheduleControls appearance="standard"
        base={base}
        template={template}
        schedules={schedules}
        canEdit={canEdit}
        canRun={canWrite}
        purchases={purchases??undefined}
      />
    </FeatureWidget></WidgetGrid></FeaturePage>
  );
}
