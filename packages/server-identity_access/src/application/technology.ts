import {provisioningInput,type ProvisioningInput,type TechnologyData} from '@erpx/server-identity_access/domain/provisioning-input';import {packageDefinition} from "@erpx/feature-people_payroll/lib/people-lifecycle";import {directoryChangeInput,type DirectoryChangeInput,type DirectoryInventory} from "@erpx/feature-identity_access/lib/directory-operations";import type {PreviewPreparation} from '@erpx/server-identity_access/domain/directory-operations';import type {IamActor} from '@erpx/server-identity_access/domain/iam-actor';import type {GrantInput} from '@erpx/server-identity_access/domain/iam-service';import type {AccessPackageDefinition} from "@erpx/feature-people_payroll/lib/people-lifecycle";import type {ProvisioningPreview} from "@erpx/server-identity_access/domain/provisioning-input";
import type {PersonPlan,ProvisionStep,AppliedPackageFact,DirectoryMembershipFact,ProvisionEngagement,ProvisionConnection,ProvisionChild,ProvisionGrant,ProvisionDirectoryPlan,BuiltProvisionPreview,ProvisionOperation} from '@erpx/server-identity_access/domain/technology';
export interface TechnologyStore{applications(facts:{actor:IamActor;observedNow:number}):Promise<AppliedPackageFact[]>;
operations(facts:{actor:IamActor}):Promise<TechnologyData["operations"]>;
parentExpiry(facts:{actor:IamActor}):Promise<{valid_until:number}|undefined>;
activeApplications(facts:{actor:IamActor}):Promise<AppliedPackageFact[]>;
membershipSources(facts:{actor:IamActor}):Promise<DirectoryMembershipFact[]>;
engagement(facts:{engagementId:string;actor:IamActor}):Promise<ProvisionEngagement|undefined>;
accessCutoff(facts:{actor:IamActor;e:ProvisionEngagement;observedNow:number}):Promise<number|null>;
restorationPlacement(facts:{actor:IamActor;e:ProvisionEngagement}):Promise<number|null>;
positionPackage(facts:{actor:IamActor;e:ProvisionEngagement}):Promise<{package_id:string;revision:number}|undefined>;
packageRevision(facts:{actor:IamActor;binding:{package_id:string;revision:number|null}}):Promise<{definition:unknown}|undefined>;
groupAvailable(facts:{actor:IamActor;groupId:string;e:ProvisionEngagement}):Promise<number|null>;
verifiedUsers(facts:{actor:IamActor;e:ProvisionEngagement;definition:AccessPackageDefinition}):Promise<{user_id:string}[]>;
roleRevision(facts:{grant:AccessPackageDefinition["grants"][number];actor:IamActor}):Promise<number|null>;
connectionInventory(facts:{connectionId:string;actor:IamActor}):Promise<ProvisionConnection|undefined>;
directoryMembers(facts:{connectionId:string;actor:IamActor;e:ProvisionEngagement}):Promise<{external_id:string}[]>;
personInProgress(facts:{actor:IamActor;plan:PersonPlan}):Promise<number|null>;
createOperation(facts:{id:string;actor:IamActor;input:ProvisioningInput;p:BuiltProvisionPreview;now:number}):Promise<void>;
applyPackage(facts:{applicationId:string;actor:IamActor;plan:PersonPlan;input:ProvisioningInput;id:string;now:number}):Promise<void>;
createGrant(facts:{grantId:string;id:string;applicationId:string;grant:GrantInput;review:{id:string}}):Promise<void>;
createDirectoryOperation(facts:{operationId:string;actor:IamActor;d:ProvisionDirectoryPlan;now:number;id:string}):Promise<void>;
saveSteps(facts:{id:string;steps:ProvisionStep[]}):Promise<void>;
lockedGrant(facts:{p:Record<string,unknown>;actor:IamActor}):Promise<ProvisionGrant|undefined>;
reviewerIsPerson(facts:{actor:IamActor;g:ProvisionGrant}):Promise<{same:boolean}>;
grantUserIsPerson(facts:{actor:IamActor;g:ProvisionGrant;grant:GrantInput}):Promise<{ok:boolean}>;
bindGrant(facts:{g:ProvisionGrant;assignment:string}):Promise<void>;
dueOperations(facts:Record<string,never>):Promise<{id:string}[]>;
sharedDirectoryLease(facts:Record<string,never>):Promise<{available:boolean}|undefined>;
lockedOperation(facts:{row:{id:string}}):Promise<ProvisionOperation|undefined>;
directoryChildren(facts:{op:ProvisionOperation}):Promise<ProvisionChild[]>;
recordChildrenState(facts:{operationId:string;status:"blocked"|"running";lastError:string|null;steps:ProvisionStep[]}):Promise<void>;
operatorAuthority(facts:{op:ProvisionOperation;observedNow:number}):Promise<number|null>;
lockedPlacement(facts:{plan:PersonPlan;op:ProvisionOperation}):Promise<{version:number;status:string;person_status:string}|undefined>;
effectiveCutoff(facts:{op:ProvisionOperation;plan:PersonPlan;observedNow:number}):Promise<{at:number|null}|undefined>;
restoredPlacement(facts:{op:ProvisionOperation;plan:PersonPlan;cutoff:number}):Promise<number|null>;
revokeOldGrants(facts:{op:ProvisionOperation;plan:PersonPlan;observedNow:number}):Promise<void>;
endPriorPackages(facts:{op:ProvisionOperation;plan:PersonPlan;observedNow:number}):Promise<void>;
restoreCutoff(facts:{op:ProvisionOperation;plan:PersonPlan;observedNow:number}):Promise<void>;
completeAccessPreparation(facts:{op:ProvisionOperation;plan:PersonPlan;now:number}):Promise<void>;
endNewApplication(facts:{applicationId:string;now:number}):Promise<void>;
endReplacedApplications(facts:{plan:PersonPlan;now:number}):Promise<void>;
retireDirectorySources(facts:{plan:PersonPlan}):Promise<void>;
trackExternalMembership(facts:{plan:PersonPlan;groupId:string;memberId:string;now:number}):Promise<void>;
trackApplicationMembership(facts:{plan:PersonPlan;groupId:string;memberId:string;applicationId:string;now:number}):Promise<void>;
createPreparationTask(facts:{applicationId:string;index:number;op:ProvisionOperation;task:{kind:string;label:string};privileged:boolean;now:number}):Promise<void>;
completeOperation(facts:{op:ProvisionOperation;steps:ProvisionStep[];observedNow:number}):Promise<void>;
appendAppliedOutbox(facts:{op:ProvisionOperation;observedNow:number}):Promise<void>;
appendAppliedAudit(facts:{generatedId:string;op:ProvisionOperation;steps:ProvisionStep[];observedNow:number}):Promise<void>;
recordBlockedOperation(facts:{row:{id:string};reason:string;observedNow:number}):Promise<void>;
retryOperation(facts:{id:string;actor:IamActor}):Promise<ProvisionOperation|undefined>;
queueRetry(facts:{id:string;actor:IamActor;observedNow:number}):Promise<void>;}
export interface TechnologyPorts{error(status:number,code:string,message:string):Error;now():number;id():string;hash(value:unknown):string;errorCode(error:unknown):string|undefined;assertCompanyScope(actor:IamActor):void;authorize(actor:IamActor,permission:string):void;fresh(actor:Pick<IamActor,'userId'|'sessionId'>):Promise<void>;parseGrant(input:unknown):GrantInput;directoryPreview(store:TechnologyStore,actor:IamActor,input:DirectoryChangeInput):Promise<PreviewPreparation>;validateGrant(store:TechnologyStore,actor:IamActor,input:GrantInput,parentId:string):Promise<unknown>;insertGrant(store:TechnologyStore,actor:IamActor,input:GrantInput,parentId:string,source:string,sourceId:string):Promise<string>;proposeReview(store:TechnologyStore,actor:IamActor,operation:string,payload:Record<string,unknown>,reason:string):Promise<{id:string}>;read<Result>(run:(store:TechnologyStore)=>Promise<Result>):Promise<Result>;preview<Result>(run:(store:TechnologyStore)=>Promise<Result>):Promise<Result>;command<Result>(actor:IamActor,key:string,operation:string,payload:unknown,permission:string,run:(store:TechnologyStore)=>Promise<Result>):Promise<Result>;due():Promise<{id:string}[]>;worker():Promise<{store:TechnologyStore;begin():Promise<void>;commit():Promise<void>;rollback():Promise<void>;release():void}>;}
export function createTechnologyService(ports:TechnologyPorts){function fail(
  message: string,
  code = "PROVISIONING_CHANGED",
  status = 409,
): never {
  throw ports.error(status, code, message);
}function scope(actor: IamActor, permission = "iam.directory.apply") {
  ports.assertCompanyScope(actor);
  if (actor.platform)
    fail("Use a Company workspace.", "COMPANY_SCOPE_REQUIRED", 403);
  ports.authorize(actor, permission);
}async function technologyData(
    actor: IamActor,
): Promise<TechnologyData> {
  scope(actor, "iam.read");

  return ports.read(async store=>{const applications = (await store.applications({actor,observedNow:ports.now()}));
  const operations = (await store.operations({actor}));
  return {
    applications: applications.map((a) => ({
      ...a,
      scope: typeof a.scope === "string" ? JSON.parse(a.scope) : a.scope,
    })),
    operations,
  };
});}
async function buildPreview(
  store:TechnologyStore,
  actor: IamActor,
  input: ProvisioningInput,
) {
  const parent = (await store.parentExpiry({actor}));
  if (
    !parent ||
    (!input.revokeOnly &&
      (input.validUntil <= ports.now() ||
        input.validUntil > Number(parent.valid_until)))
  )
    fail("Choose an expiry within your delegated authority.");
  const plans: PersonPlan[] = [],
    changes: DirectoryChangeInput[] = [],
    inventories = new Map<
      string,
      { version: number; inventory: DirectoryInventory }
    >();
  const beforeApps = (await store.activeApplications({actor}));
  const sources = (await store.membershipSources({actor}));
  for (const engagementId of input.engagementIds) {
    const e = (await store.engagement({engagementId,actor}));
    if (
      !e ||
      (!input.revokeOnly &&
        (e.status !== "active" || e.person_status !== "active"))
    )
      fail(
        "Complete HR placement before applying employee resources.",
        "PLACEMENT_REQUIRED",
      );
    const cutoff = (await store.accessCutoff({actor,e,observedNow:ports.now()}));
    if (cutoff && !input.revokeOnly) {
      if (
        !input.restoreAccess ||
        !(await store.restorationPlacement({actor,e}))
      )
        fail(
          "Confirm restoration after HR completes a new placement. Old grants will be revoked permanently.",
          "ACCESS_CUTOFF_ACTIVE",
        );
    }
    const binding = input.packageId
      ? { package_id: input.packageId, revision: input.revision }
      : (await store.positionPackage({actor,e}));
    if (!binding?.revision)
      fail(
        "Publish and bind an access package, or select a published package.",
        "PACKAGE_REQUIRED",
      );
    const revision = (await store.packageRevision({actor,binding}));
    if (!revision) fail("Choose a published package revision.");
    const recipe = packageDefinition.parse(revision.definition);
    let definition = {
      ...recipe,
      grants: recipe.grants.filter(
        (g) => !input.exclude.includes("role:" + g.roleId),
      ),
      groupIds: recipe.groupIds.filter(
        (id) => !input.exclude.includes("group:" + id),
      ),
      directoryGroupIds: recipe.directoryGroupIds.filter(
        (id) => !input.exclude.includes("directory:" + id),
      ),
      windowsPolicies: recipe.windowsPolicies.filter(
        (p) => !input.exclude.includes("gpo:" + p.id),
      ),
    };
    if (input.revokeOnly)
      definition = {
        ...definition,
        grants: [],
        groupIds: [],
        directoryGroupIds: [],
        windowsPolicies: [],
        tasks: [],
      };
    for (const groupId of definition.groupIds) {
      if (
        !(await store.groupAvailable({actor,groupId,e}))
      )
        fail("An ERPX group is unavailable or outside this employee's scope.");
    }
    const users = (await store.verifiedUsers({actor,e,definition})).map((r) => String(r.user_id));
    if (definition.grants.length && !users.length)
      fail(
        "Verify and link the account before requesting ERPX roles.",
        "ACCOUNT_REQUIRED",
      );
    for (const userId of users)
      for (const grant of definition.grants) {
        const requested = ports.parseGrant({
          ...grant,
          userId,
          validUntil: Math.min(grant.validUntil, input.validUntil),
          reason: input.reason,
        });
        if (requested.validUntil <= ports.now())
          fail(
            "The package contains an expired role grant. Publish an updated revision.",
          );
        const role = (await store.roleRevision({grant,actor}));
        if (!role)
          fail(
            "The requested role revision changed. Update the package first.",
          );
      }
    const old = input.revokeOnly
      ? beforeApps.filter(
          (a) =>
            a.person_id === e.person_id &&
            input.revokeApplicationIds.includes(a.id),
        )
      : input.sourceKind === "position"
        ? beforeApps.filter(
            (a) => a.person_id === e.person_id && a.source_kind === "position",
          )
        : [];
    if (input.revokeOnly && !old.length)
      fail("The selected applied source changed.");
    const retained = beforeApps
      .filter((a) => a.person_id === e.person_id && !old.includes(a))
      .flatMap((a) => [a.definition.name + " · " + a.source_kind]);
    const directoryMembers: string[] = [];
    const connectionIds = [
      ...new Set(
        [
          definition.connectionId,
          ...old.map((a) => a.definition.connectionId),
        ].filter((v): v is string => !!v),
      ),
    ];
    for (const connectionId of connectionIds) {
      let state = inventories.get(connectionId);
      if (!state) {
        const row = (await store.connectionInventory({connectionId,actor}));
        if (
          !row?.inventory ||
          row.status !== "active" ||
          !row.write_enabled ||
          row.tested_version !== row.version
        )
          fail(
            "Refresh and enable this connection before provisioning.",
            "DIRECTORY_READ_ONLY",
          );
        state = { version: row.version, inventory: row.inventory };
        inventories.set(connectionId, state);
      }
      const members = (await store.directoryMembers({connectionId,actor,e})).map((r) => String(r.external_id));
      const wanted =
        definition.connectionId === connectionId
          ? definition.directoryGroupIds
          : [];
      if (wanted.length && !members.length)
        fail(
          "Verify and link the directory account first.",
          "ACCOUNT_REQUIRED",
        );
      if (connectionId === definition.connectionId)
        directoryMembers.push(...members);
      for (const groupId of wanted) {
        const group = state.inventory.objects.find(
          (o) => o.id === groupId && o.kind === "group",
        );
        if (!group) fail("Refresh the package's directory groups.");
        const missing = members.filter((id) => !group.memberIds.includes(id));
        if (missing.length)
          changes.push(
            directoryChangeInput.parse({
              connectionId,
              expectedVersion: state.version,
              action: {
                kind: "group.members",
                objectId: groupId,
                memberIds: missing,
                action: "add",
              },
              reason: input.reason,
            }),
          );
      }
      for (const groupId of [
        ...new Set(
          old
            .filter((a) => a.definition.connectionId === connectionId)
            .flatMap((a) => a.definition.directoryGroupIds as string[]),
        ),
      ]) {
        if (wanted.includes(groupId)) continue;
        // Retire every replaced package source under the Directory worker's
        // connection lock, including memberships another source still needs.
        const ownedMembers = members.filter(
          (memberId) =>
            sources.some(
              (s) =>
                s.connection_id === connectionId &&
                s.group_id === groupId &&
                s.member_id === memberId &&
                old.some((a) => a.id === s.application_id),
            ),
        );
        const removable = ownedMembers.filter(
          (memberId) =>
            !sources.some(
              (s) =>
                s.connection_id === connectionId &&
                s.group_id === groupId &&
                s.member_id === memberId &&
                !old.some((a) => a.id === s.application_id),
            ),
        );
        if (ownedMembers.length)
          changes.push(
            directoryChangeInput.parse({
              connectionId,
              expectedVersion: state.version,
              action: {
                kind: "group.members",
                objectId: groupId,
                memberIds: ownedMembers,
                action: "remove",
              },
              reason: input.reason,
            }),
          );
        if (members.some((id) => !removable.includes(id)))
          retained.push(
            "Directory membership from another or untracked source · " +
              groupId,
          );
      }
    }
    plans.push({
      id: e.id,
      personId: e.person_id,
      name: e.legal_name,
      positionId: e.position_id,
      branchId: e.branch_id,
      departmentId: e.department_id,
      engagementVersion: e.version,
      packageId: binding.package_id,
      revision: binding.revision,
      definition,
      oldIds: old.map((a) => a.id),
      users,
      directoryMembers,
      retained,
    });
  }
  // One operation per group/action avoids stale target versions within a bulk request.
  const grouped = new Map<string, DirectoryChangeInput>();
  for (const change of changes) {
    const a = change.action;
    if (a.kind !== "group.members") continue;
    const k = change.connectionId + ":" + a.objectId + ":" + a.action;
    const old = grouped.get(k);
    if (old?.action.kind === "group.members")
      old.action.memberIds = [
        ...new Set([...old.action.memberIds, ...a.memberIds]),
      ];
    else grouped.set(k, change);
  }
  const touched = [...grouped.values()].map(
    (c) =>
      c.connectionId + ":" + ("objectId" in c.action ? c.action.objectId : ""),
  );
  if (new Set(touched).size !== touched.length)
    fail(
      "Split additions and removals to the same group into separate previews.",
      "DIRECTORY_BATCH_DEPENDENCY",
    );
  const directory = [];
  for (const change of grouped.values()) {
    if (
      change.action.kind === "group.members" &&
      change.action.memberIds.length > 100
    )
      fail("Split this request into smaller employee batches.");
    const p = await ports.directoryPreview(store, actor, change);
    if (p.evidence.companies.some((id) => id !== actor.companyId))
      fail(
        "Use the Directory console to review changes shared across Companies.",
        "DIRECTORY_APPLY_SCOPE",
        403,
      );
    directory.push({
      change,
      evidence: p.evidence,
      stateVersion: p.state.version,
    });
  }
  if (
    input.revokeOnly &&
    input.revokeApplicationIds.some(
      (id) => !plans.some((p) => p.oldIds.includes(id)),
    )
  )
    fail("Every withdrawn source must belong to the selected employees.");
  const evidence = {
    plans,
    directory,
    inventories: [...inventories.entries()],
    sources,
    beforeApps: beforeApps.map((a) => [a.id, a.ended_at, a.valid_until]),
  };
  const result: ProvisioningPreview = {
    token: ports.hash({ input, evidence }),
    employees: plans.map((p) => ({
      id: p.id,
      name: p.name,
      packageName: p.definition.name,
      revision: p.revision,
      added: [
        ...p.definition.grants.map(
          (g) => "ERPX role (independent review): " + g.roleId,
        ),
        ...p.definition.groupIds.map((id) => "ERPX group: " + id),
        ...p.definition.directoryGroupIds.map((id) => "Directory group: " + id),
      ],
      removed: p.oldIds.flatMap((id) => {
        const old = beforeApps.find((a) => a.id === id)!;
        return [
          old.definition.name + " · applied revision " + old.package_revision,
        ];
      }),
      retained: p.retained,
      reviews: p.users.length * p.definition.grants.length,
    })),
    warnings: [
      ...(input.restoreAccess
        ? [
            "Restore access after HR placement. All old grants remain revoked; requested ERPX roles need new independent review.",
          ]
        : []),
      "Only the selected employees receive this revision. ERPX roles require an independent reviewer.",
      ...(plans.some((p) => p.definition.windowsPolicies.length)
        ? [
            "GPO targets are shared. Publish or link GPOs separately in Directory & GPO after reviewing all affected people and computers.",
          ]
        : []),
    ],
  };
  return { result, evidence };
}
async function previewProvisioning(actor:IamActor,input:ProvisioningInput){scope(actor);return ports.preview(async store=>(await buildPreview(store,actor,provisioningInput.parse(input))).result);}
async function applyProvisioning(
    actor: IamActor,
  input: ProvisioningInput,
  token: string,
  key: string,
) {
  scope(actor);
  await ports.fresh(actor);
  input = provisioningInput.parse(input);
  return ports.command(
    actor,
    key,
    "technology.provision.apply",
    input,
    "iam.directory.apply",
    async (store) => {
      const p = await buildPreview(store, actor, input);
      if (p.result.token !== token)
        fail(
          "Review the current employee and resource changes first.",
          "PREVIEW_CHANGED",
        );
      for (const plan of p.evidence.plans)
        if (
          (await store.personInProgress({actor,plan}))
        )
          fail("This employee already has provisioning in progress.");
      const id = ports.id(),
        now = ports.now();
      (await store.createOperation({id,actor,input,p,now}));
      const reviews: string[] = [],
        steps: { id: string; status: string; label: string }[] = [];
      for (const plan of p.evidence.plans) {
        const applicationId = id + ":" + plan.id;
        (await store.applyPackage({applicationId,actor,plan,input,id,now}));
        for (const userId of plan.users)
          for (const [index, g] of plan.definition.grants.entries()) {
            const grant = ports.parseGrant({
                ...g,
                userId,
                validUntil: Math.min(g.validUntil, input.validUntil),
                reason: input.reason,
              }),
              grantId = applicationId + ":" + userId + ":" + index;
            const review = await ports.proposeReview(
              store,
              actor,
              "people.provision.grant",
              {
                ...grant,
                operationId: id,
                grantId,
                parentId: actor.assignmentId,
              },
              input.reason,
            );
            (await store.createGrant({grantId,id,applicationId,grant,review}));
            reviews.push(review.id);
          }
      }
      for (const [index, d] of p.evidence.directory.entries()) {
        const operationId = id + ":directory:" + index;
        (await store.createDirectoryOperation({operationId,actor,d,now,id}));
        steps.push({
          id: operationId,
          status: "queued",
          label:
            d.change.action.kind +
            " · " +
            ("objectId" in d.change.action ? d.change.action.objectId : ""),
        });
      }
      (await store.saveSteps({id,steps}));
      return { id, status: "queued", reviews };
    },
  );
}
async function applyProvisioningReview(
  store:TechnologyStore,
  actor: IamActor,
  p: Record<string, unknown>,
  decision = "approve",
) {
  const g = (await store.lockedGrant({p,actor}));
  if (
    !g ||
    g.assignment_id ||
    g.ended_at ||
    ["rejected", "blocked"].includes(g.status)
  )
    fail("This applied access request changed.");
  if (decision === "reject") return; // Refusing a role never undoes HR placement or unrelated resources.
  if (
    (await store.reviewerIsPerson({actor,g})).same
  )
    fail("Another person must review your access.", "SELF_ACCESS_CHANGE", 403);
  const grant = ports.parseGrant(g.definition);
  if (
    !(await store.grantUserIsPerson({actor,g,grant})).ok
  )
    fail("The verified account link changed.");
  await ports.validateGrant(store, actor, grant, actor.assignmentId);
  const assignment = await ports.insertGrant(
    store,
    actor,
    grant,
    actor.assignmentId,
    "provisioning",
    g.id,
  );
  (await store.bindGrant({g,assignment}));
}
async function drainProvisioning() {
  const due = (await ports.due());
  for (const row of due) {
    const session=await ports.worker();const store=session.store;
    try {
      await session.begin();
      // Keep package source retirement atomic with the GPO reconciler's final
      // source check. A busy Directory pass defers this tick without failing it.
      const available = (await store.sharedDirectoryLease({}))?.available;
      if (!available) {
        await session.commit();
        break;
      }
      const op = (await store.lockedOperation({row}));
      if (!op) {
        await session.rollback();
        continue;
      }
      const children = (await store.directoryChildren({op}));
      const steps = op.steps.map((s: { id: string }) => ({
        ...s,
        status: children.find((j) => j.id === s.id)?.status ?? "blocked",
      }));
      if (children.some((j) => j.status !== "completed")) {
        (await store.recordChildrenState({operationId:op.id,status:children.some(j=>j.status==="blocked")?"blocked":"running",lastError:children.find(j=>j.last_error)?.last_error??null,steps}));
        await session.commit();
        continue;
      }
      await ports.fresh({
        userId: op.created_by,
        sessionId: op.session_id,
      });
      if (
        !(await store.operatorAuthority({op,observedNow:ports.now()}))
      )
        fail(
          "The IT operator's authority changed.",
          "PROVISIONING_AUTHORITY_CHANGED",
        );
      for (const plan of op.evidence.plans as PersonPlan[]) {
        const current = (await store.lockedPlacement({plan,op}));
        if (
          !current ||
          current.version !== plan.engagementVersion ||
          (!op.definition.revokeOnly &&
            (current.status !== "active" || current.person_status !== "active"))
        )
          fail("HR placement changed. Preview provisioning again.");
        const cutoff = (await store.effectiveCutoff({op,plan,observedNow:ports.now()}))?.at;
        if (cutoff && !op.definition.revokeOnly) {
          if (
            !op.definition.restoreAccess ||
            !(await store.restoredPlacement({op,plan,cutoff}))
          )
            fail("This employee's access was suspended.");
          (await store.revokeOldGrants({op,plan,observedNow:ports.now()}));
          (await store.endPriorPackages({op,plan,observedNow:ports.now()}));
          (await store.restoreCutoff({op,plan,observedNow:ports.now()}));
        }
        const now = ports.now(),
          applicationId = op.id + ":" + plan.id;
        if (!op.definition.revokeOnly)
          (await store.completeAccessPreparation({op,plan,now}));
        if (op.definition.revokeOnly)
          (await store.endNewApplication({applicationId,now}));
        // Retire only the replaced source; manual and supplemental assignments keep their own expiry.
        (await store.endReplacedApplications({plan,now}));
        (await store.retireDirectorySources({plan}));
        for (const groupId of plan.definition.directoryGroupIds)
          for (const memberId of plan.directoryMembers) {
            const wasMember = op.evidence.inventories.some(
              ([id, state]: [string, { inventory: DirectoryInventory }]) =>
                id === plan.definition.connectionId &&
                state.inventory.objects.some(
                  (o) => o.id === groupId && o.memberIds.includes(memberId),
                ),
            );
            const tracked = op.evidence.sources.some(
              (s: {
                connection_id: string;
                group_id: string;
                member_id: string;
              }) =>
                s.connection_id === plan.definition.connectionId &&
                s.group_id === groupId &&
                s.member_id === memberId,
            );
            if (wasMember && !tracked)
              (await store.trackExternalMembership({plan,groupId,memberId,now}));
            (await store.trackApplicationMembership({plan,groupId,memberId,applicationId,now}));
          }
        for (const [index, task] of [
          ...plan.definition.tasks,
          ...plan.definition.windowsPolicies.map((p) => ({
            kind: "directory",
            label: "Verify shared GPO · " + p.name,
          })),
        ].entries())
          (await store.createPreparationTask({applicationId,index,op,task,privileged:!["equipment","handover"].includes(task.kind),now}));
      }
      (await store.completeOperation({op,steps,observedNow:ports.now()}));
      (await store.appendAppliedOutbox({op,observedNow:ports.now()}));
      (await store.appendAppliedAudit({generatedId:ports.id(),op,steps,observedNow:ports.now()}));
      await session.commit();
    } catch (error) {
      await session.rollback();
      (await store.recordBlockedOperation({row,reason:ports.errorCode(error)??"PROVISIONING_RETRY_REQUIRED",observedNow:ports.now()}));
    } finally {
      session.release();
    }
  }
}
async function retryProvisioning(
    actor: IamActor,
  id: string,
  version: number,
  key: string,
) {
  scope(actor);
  await ports.fresh(actor);
  return ports.command(
    actor,
    key,
    "technology.provision.retry",
    { id, version },
    "iam.directory.apply",
    async (store) => {
      const op = (await store.retryOperation({id,actor}));
      if (
        !op ||
        op.created_by !== actor.userId ||
        op.version !== version ||
        !["retry", "blocked"].includes(op.status) ||
        !["FRESH_MFA_REQUIRED", "PROVISIONING_RETRY_REQUIRED"].includes(
          op.last_error ?? "",
        )
      )
        fail(
          "Read the current state and preview a new change. Only an interrupted operation can resume.",
        );
      (await store.queueRetry({id,actor,observedNow:ports.now()}));
      return { id, status: "queued" };
    },
  );
}
return {technologyData,previewProvisioning,applyProvisioning,applyProvisioningReview,drainProvisioning,retryProvisioning};}
