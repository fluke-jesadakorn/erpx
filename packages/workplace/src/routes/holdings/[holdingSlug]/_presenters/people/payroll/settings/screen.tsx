import {PayrollPostingSettingsLink} from '@erpx/feature-records/components/loaders/modules--payroll-posting-prerequisites';
import {PayrollEnvironment} from "@erpx/feature-people_payroll/components/payroll-environment";
import {payrollPurchaseControls} from "@erpx/server-people_payroll/public/payroll-purchases";
import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import { ActionLink as UiActionLink } from "@erpx/ui/components/foundation";
import { PayrollRunEditor } from "@erpx/feature-people_payroll/components/payroll-controls";
import { PayrollAccountMappingSettings } from "@erpx/feature-people_payroll/components/payroll-accounting-panel";
import { PayrollScheduleSettings } from "@erpx/feature-people_payroll/components/payroll-schedule-settings";
import { PayrollStatutorySettings } from "@erpx/feature-people_payroll/components/payroll-statutory-settings";
import {
  PayrollAlert,
  PayrollHeading,
  PayrollNote,
  PayrollPageIntro,
} from "@erpx/feature-people_payroll/components/payroll-workspace";
import { hasPermission } from "@erpx/feature-identity_access/lib/permissions";
import { workspaceBase } from "@erpx/feature-work_projection/lib/workspace";
import { publicScope, requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { pageError } from "@erpx/server-work_projection/public/page-error";
import { listCountryPacks,payrollOptions, payrollPostingOptions } from "@erpx/server-people_payroll/public/people-payroll";
import { thaiPayrollConfiguration } from "@erpx/server-people_payroll/public/payroll-statutory";
import { getPayrollSchedule } from "@erpx/server-people_payroll/public/payroll-readiness";
import { isThaiHotelPayrollMockEnabled } from "@erpx/server-runtime/public/simulation-only-adapters";
import { forbidden } from "next/navigation";
import { PageContent, Card } from "@erpx/ui/components/foundation";

export const dynamic = "force-dynamic";

export default async function PayrollSettings({
  params,
}: {
  params: Promise<{ holdingSlug: string }>;
}) {
  const { holdingSlug } = await params;
  const actor = await requireWorkContext(holdingSlug);
  const canPrepare = hasPermission(actor, "payroll.prepare"),
    canMap = hasPermission(actor, "finance.approve"),canReview=hasPermission(actor,"people.compensation.approve"),canReadRules=hasPermission(actor,"payroll.configuration.read");
  if ((!canPrepare && !canMap && !canReadRules) || actor.branchId || actor.departmentId) forbidden();
  const purchase = await payrollPurchaseControls(actor);
  const base = workspaceBase(publicScope(actor));
  let groups: Awaited<ReturnType<typeof payrollOptions>> = [];
  let postingOptions: Awaited<ReturnType<typeof payrollPostingOptions>> | null = null;
  let schedules: Awaited<ReturnType<typeof getPayrollSchedule>>[] = [];
  let mappingUnavailable = false,
    scheduleUnavailable = false;
  const [packs,statutory]=canReadRules?await Promise.all([listCountryPacks(actor),thaiPayrollConfiguration(actor)]):[[],null];
  try {
    if (canPrepare) groups = await payrollOptions(actor);
  } catch (error) {
    return pageError(error, actor.holdingSlug);
  }
  if (canMap) {
    try {
      postingOptions = await payrollPostingOptions(actor);
      if (!canPrepare) groups = postingOptions.groups;
    } catch {
      mappingUnavailable = true;
    }
  }
  if (canPrepare) {
    const results = await Promise.all(
      groups.map((group) => getPayrollSchedule(actor, String(group.id)).catch(() => null)),
    );
    scheduleUnavailable = results.some((value) => value === null);
    schedules = results.filter((value): value is NonNullable<typeof value> => value !== null);
  }
  return (
    <PayrollEnvironment purchase={purchase}><PageContent as="div" className="ui-page-content" data-source="live">
      <PayrollPageIntro
        base={base}
        title="Settings"
        description="Manage scheduled runs and accounting. Pay group policies are configured in People."
      />
      {canPrepare && (
        <p>
          <UiActionLink appearance="standard"
            variant="secondary"
            href={canonicalUiHref(`${base}/people?panel=settings&section=pay-groups`)}
          >
            Manage pay groups in People
          </UiActionLink>
        </p>
      )}
      {scheduleUnavailable && (
        <PayrollAlert message="Payroll schedules could not load. Ask an administrator to check the Payroll database migration." />
      )}
      {canPrepare && (
        <PayrollScheduleSettings
          base={base}
          userId={actor.userId}
          schedules={schedules}
          names={Object.fromEntries(groups.map((group) => [String(group.id), String(group.name)]))}
        />
      )}
      {canMap&&<p><PayrollPostingSettingsLink href={canonicalUiHref(`${base}/people-organization/people/payroll/settings/posting`)}/></p>}
      {mappingUnavailable && (
        <PayrollAlert message="Payroll account mappings could not load. Ask an administrator to check the Finance database migration." />
      )}
      {canMap && postingOptions && (
        <PayrollAccountMappingSettings
          base={base}
          groups={groups}
          options={postingOptions}
          userId={actor.userId}
        />
      )}
      {statutory&&<PayrollStatutorySettings base={base} packs={packs} profiles={statutory.profiles} employees={statutory.employees} canPrepare={canPrepare} canReview={canReview}/>}
      {canPrepare && (
          <Card as="section" appearance="standard" className="ui-panel">
            <PayrollHeading title="Manual run" />
            <PayrollNote text="Create a gross pay preview or calculate under an independently reviewed statutory revision." />
            <PayrollRunEditor appearance="standard"
              base={base}
              userId={actor.userId}
              groups={groups}
              packs={packs}
              simulationMode={isThaiHotelPayrollMockEnabled(actor.holdingSlug)}
            />
          </Card>
        )}
    </PageContent></PayrollEnvironment>
  );
}
