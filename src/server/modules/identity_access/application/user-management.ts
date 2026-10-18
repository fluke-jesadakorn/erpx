import {managementChange,attributeAccess,personRuleFields,ruleMatches,type ManagementChange,type ManagementSnapshot,type AttributeDefinition,type LocalGroup,type AccessPolicy,type PolicyAttachment} from "@/lib/identity_access/user-management";import {hasPermission} from "@/lib/identity_access/permissions";import type {IamActor} from '../domain/iam-actor';import {grantInput,type GrantInput} from '../domain/iam-service';import {fail,checkedAttributeValue,commandPermission,managementImpact,type ManagementObjectKind} from '../domain/user-management';import type {IdentityError} from './authorization';
export type ManagementObject<Definition>={id:string;version:number;definition:Definition;author_id?:string;parent_id?:string};
export type ManagementValueRow={company_id:string;target:'person'|'group'|'unit';object_id:string;version:number;definition:Record<string,unknown>};
export type ManagementCandidate={id:string;company_id:string;person_id:string;user_id:string;attachment_id:string;definition:GrantInput&{blockedReason?:string};status:string;review_id:string|null;review_status:string|null;review_version:number|null;assignment_id:string|null;enabled:boolean};
export type ManagementPolicyMatch={id:string;parent_id:string;author_id:string;version:number;policy_version:number;policy_data:AccessPolicy|string;person_id:string;user_id:string};
export type ManagementProposer={id:string;scope_kind:string;scope_node_id:string|null;role:string;grantable_json:string[]};

export interface UserManagementStore{activeScopeNode(facts:{actor:IamActor;node:string|null}):Promise<number|null>;
descendantScope(facts:{actor:IamActor;node:string|null}):Promise<{ok:boolean}>;
person(facts:{id:string;actor:IamActor}):Promise<{id:string;legal_name:string;user_id:string|null}|null>;
personInScope(facts:{actor:IamActor;id:string}):Promise<{ok:boolean}>;
group(facts:{actor:IamActor;id:string}):Promise<ManagementObject<LocalGroup>|null>;
groups(facts:{company:string;scope:string|null}):Promise<ManagementObject<LocalGroup>[]>;
people(facts:{company:string;scope:string|null}):Promise<{id:string;name:string;user_id:string|null;definition:Record<string,unknown>;groups_json:(string|null)[]}[]>;
memberships(facts:{company:string}):Promise<{id:string;person_id:string;direct:boolean;rule:boolean;nested:boolean;position:boolean}[]>;
nodes(facts:{company:string;scope:string|null}):Promise<{id:string;parent_id:string|null;display_name:string;kind:string}[]>;
attachments(facts:{company:string}):Promise<ManagementObject<PolicyAttachment>[]>;
policies(facts:{company:string}):Promise<ManagementObject<AccessPolicy>[]>;
attributes(facts:{company:string}):Promise<ManagementObject<AttributeDefinition>[]>;
values(facts:{company:string}):Promise<ManagementValueRow[]>;
ancestry(facts:{company:string}):Promise<{id:string;parent_id:string|null}[]>;
candidates(facts:{company:string;observedNow:number}):Promise<ManagementCandidate[]>;
lockObjectVersion(facts:{table:ManagementObjectKind;actor:IamActor;id:string}):Promise<{version:number}|null>;
replaceOwnedAttachment(facts:{table:ManagementObjectKind;actor:IamActor;id:string;version:number;definition:unknown;extra:{authorId:string;parentId:string}}):Promise<void>;
replaceObject(facts:{table:ManagementObjectKind;actor:IamActor;id:string;version:number;definition:unknown}):Promise<void>;
appendObjectHistory(facts:{actor:IamActor;table:ManagementObjectKind;id:string;version:number;definition:unknown;observedNow:number}):Promise<void>;
parentExpiry(facts:{actor:IamActor}):Promise<{valid_until:number}>;
grantScopeNode(facts:{actor:IamActor;grant:AccessPolicy["grants"][number]}):Promise<number|null>;
publishedGrantRole(facts:{actor:IamActor;grant:AccessPolicy["grants"][number]}):Promise<{permissions_json:string[];grantable_json:string[];scopes_json:string[]}|null>;
attributeRoleActive(facts:{roleId:string;actor:IamActor}):Promise<number|null>;
attributeRuleUse(facts:{actor:IamActor;input:{id?:string}}):Promise<number|null>;
parentGroupScope(facts:{actor:IamActor;input:{scopeNodeId:string|null};row:ManagementObject<LocalGroup>}):Promise<{ok:boolean}>;
childGroupScope(facts:{actor:IamActor;child:LocalGroup;input:{scopeNodeId:string|null}}):Promise<{ok:boolean}>;
memberInGroupScope(facts:{actor:IamActor;personId:string;row:ManagementObject<LocalGroup>}):Promise<{ok:boolean}>;
addMember(facts:{actor:IamActor;input:{groupId:string};personId:string}):Promise<void>;
removeMember(facts:{actor:IamActor;input:{groupId:string};personId:string}):Promise<void>;
lockValues(facts:{actor:IamActor;input:{target:string;objectId:string}}):Promise<ManagementValueRow|null>;
inheritedValue(facts:{input:{objectId:string};actor:IamActor;fieldId:string}):Promise<{value:unknown}|null>;
replaceValues(facts:{actor:IamActor;input:{target:string;objectId:string};version:number;values:Record<string,unknown>}):Promise<void>;
appendValueHistory(facts:{actor:IamActor;input:{target:string;objectId:string};version:number;values:Record<string,unknown>;observedNow:number}):Promise<void>;
exclusionWithinTarget(facts:{actor:IamActor;excluded:string;input:{targetId:string}}):Promise<{ok:boolean}>;
lockManagement(facts:{companyId:string}):Promise<void>;
rejectRevokedCandidates(facts:{companyId:string}):Promise<void>;
invalidateManagedAccess(facts:{companyId:string}):Promise<void>;
rejectReviewedCandidates(facts:{companyId:string}):Promise<void>;
eligiblePolicyMatches(facts:{companyId:string;observedNow:number}):Promise<ManagementPolicyMatch[]>;
policyProposer(facts:{match:ManagementPolicyMatch}):Promise<ManagementProposer|null>;
existingCandidate(facts:{companyId:string;match:ManagementPolicyMatch;index:number}):Promise<number|null>;
recordBlockedCandidate(facts:{generatedId:string;companyId:string;match:ManagementPolicyMatch;index:number;grant:GrantInput;blocked:string;observedNow:number}):Promise<void>;
insertGrantReview(facts:{reviewId:string;companyId:string;payload:GrantInput&{candidateId:string;parentId:string;authorizingAssignmentId:string};match:ManagementPolicyMatch;observedNow:number;grant:GrantInput}):Promise<void>;
insertGrantCandidate(facts:{id:string;companyId:string;match:ManagementPolicyMatch;index:number;grant:GrantInput;reviewId:string;observedNow:number}):Promise<void>;
appendGrantProposalAudit(facts:{generatedId:string;companyId:string;match:ManagementPolicyMatch;id:string;payload:GrantInput&{candidateId:string;parentId:string;authorizingAssignmentId:string};observedNow:number}):Promise<void>;
lockCandidate(facts:{actor:IamActor;p:Record<string,unknown>}):Promise<ManagementCandidate|null>;
candidateEligible(facts:{row:ManagementCandidate;observedNow:number}):Promise<{ok:boolean}>;
activateCandidate(facts:{assignmentId:unknown;row:ManagementCandidate}):Promise<void>;
lockCommand(facts:{actor:IamActor}):Promise<void>;}
export interface UserManagementPorts{error:IdentityError;now():number;id():string;hash(value:unknown):string;authorize(actor:IamActor,permission:string):unknown;fresh(actor:IamActor):Promise<void>;ruleError(error:unknown):error is Error&{code:string};readTransaction<Result>(run:(store:UserManagementStore)=>Promise<Result>):Promise<Result>;previewTransaction<Result>(run:(store:UserManagementStore)=>Promise<Result>):Promise<Result>;command<Result>(actor:IamActor,key:string,operation:string,payload:unknown,permission:string,run:(store:UserManagementStore)=>Promise<Result>):Promise<Result>;workSession(run:(session:{companies():Promise<string[]>;transaction(companyId:string,run:(store:UserManagementStore)=>Promise<void>):Promise<void>})=>Promise<void>):Promise<void>;validateGrant(store:UserManagementStore,actor:IamActor,input:GrantInput,parentId?:string):Promise<unknown>;insertGrant(store:UserManagementStore,actor:IamActor,input:GrantInput,parentId:string,source:string,sourceId:string):Promise<unknown>;}
export function createUserManagementApplication(ports:UserManagementPorts){const allowedScope = async (
  store:UserManagementStore,
  actor: IamActor,
  node: string | null,
) => {
  if (actor.platform)
    fail(
      "User management is available in Company workspaces.",
      "SCOPE_FORBIDDEN",
      403,
    );
  if (
    node &&
    !(await store.activeScopeNode({actor,node}))
  )
    fail("Choose an active unit in this Company.");
  if (
    actor.scopeKind !== "holding" &&
    (!node ||
      !(await store.descendantScope({actor,node})).ok)
  )
    fail("This object is outside your workspace.", "SCOPE_FORBIDDEN", 403);
};
const visiblePerson = async (store:UserManagementStore, actor: IamActor, id: string) => {
  const p = (await store.person({id,actor}));
  if (
    !p ||
    (actor.scopeKind !== "holding" &&
      !(await store.personInScope({actor,id})).ok)
  )
    fail("This person is outside your workspace.", "SCOPE_FORBIDDEN", 403);
  return p;
};
async function groupRow(store:UserManagementStore, actor: IamActor, id: string) {
  const row = (await store.group({actor,id}));
  if (!row) fail("Choose a local group.");
  await allowedScope(store, actor, row.definition.scopeNodeId);
  return row;
}
async function managementSnapshot(
  store:UserManagementStore,
  actor: IamActor,
): Promise<ManagementSnapshot> {
  ports.authorize(actor, "iam.read");
  if (actor.platform)
    fail("Choose a Company workspace.", "SCOPE_FORBIDDEN", 403);
  const company = actor.companyId,
    scope = actor.scopeKind === "holding" ? null : actor.scopeNodeId;
  const groups = (await store.groups({company,scope})).map(
    (r) =>
      ({
        ...r.definition,
        id: r.id,
        version: r.version,
        members: [],
      }) as LocalGroup,
  );
  const people = (await store.people({company,scope})).map((r) => ({
    id: r.id,
    name: r.name,
    userId: r.user_id,
    facts: r.definition,
    nodes: r.groups_json.filter((id): id is string => Boolean(id)),
  }));
  const memberships = (await store.memberships({company}));
  for (const g of groups)
    g.members = memberships
      .filter((m) => m.id === g.id && people.some((p) => p.id === m.person_id))
      .map((m) => m.person_id);
  for (const g of groups)
    g.memberSources = Object.fromEntries(
      memberships
        .filter((m) => m.id === g.id && g.members.includes(m.person_id))
        .map((m) => [
          m.person_id,
          [
            m.direct ? "Added directly" : null,
            m.rule ? "Rule" : null,
            m.nested ? "Nested group" : null,
            m.position ? "Position access package" : null,
          ].filter((source): source is string => Boolean(source)),
        ]),
    );
  const nodes = (await store.nodes({company,scope})).map((r) => ({
    id: r.id,
    parentId: r.parent_id,
    label: r.display_name,
    kind: r.kind,
  }));
  const attachments = (await store.attachments({company}))
    .filter((r) =>
      r.definition.target === "group"
        ? groups.some((g) => g.id === r.definition.targetId)
        : nodes.some((n) => n.id === r.definition.targetId),
    )
    .map(
      (r) =>
        ({ ...r.definition, id: r.id, version: r.version }) as PolicyAttachment,
    );
  const policies = (await store.policies({company}))
    .filter(
      (r) =>
        actor.scopeKind === "holding" ||
        r.definition.grants.every((g: { scopeNodeId: string | null }) =>
          nodes.some((n) => n.id === g.scopeNodeId),
        ),
    )
    .map(
      (r) =>
        ({ ...r.definition, id: r.id, version: r.version }) as AccessPolicy,
    );
  const allAttributes = (await store.attributes({company})).map(
    (r) =>
      ({
        ...r.definition,
        id: r.id,
        version: r.version,
      }) as AttributeDefinition,
  );
  const attributes = allAttributes
    .map((a) => {
      const access = attributeAccess(a, actor.role, actor.scopeKind);
      const companyIt = a.target === "person" && !a.ownerRole && actor.role === "it_admin" && actor.scopeKind === "holding" && hasPermission(actor, "people.custom_attributes.write");
      return { ...a, canView: access.canView || companyIt, canEdit: companyIt || access.canEdit && (a.target !== "person" || hasPermission(actor, "people.custom_attributes.write")) };
    })
    .filter((a) => a.canView || hasPermission(actor, "iam.attributes.manage"));
  const visibleFields = new Set(
    attributes.filter((a) => a.canView).map((a) => a.id),
  );
  for (const person of people)
    person.facts = Object.fromEntries(
      Object.entries(person.facts).filter(
        ([key]) =>
          !key.startsWith("custom:") || visibleFields.has(key.slice(7)),
      ),
    );
  for (const group of groups)
    if (
      group.rule?.conditions.some(
        (c) =>
          c.field.startsWith("custom:") && !visibleFields.has(c.field.slice(7)),
      )
    )
      group.rule = null;
  const allValues = (await store.values({company}));
  const values: ManagementSnapshot["values"] = allValues
    .filter((r) =>
      r.target === "person"
        ? people.some((p) => p.id === r.object_id)
        : r.target === "group"
          ? groups.some((g) => g.id === r.object_id)
          : nodes.some((n) => n.id === r.object_id),
    )
    .map((r) => ({
      target: r.target,
      objectId: r.object_id,
      values: Object.fromEntries(
        Object.entries(r.definition).filter(([key]) => visibleFields.has(key)),
      ),
      version: r.version,
      expectedVersion: r.version,
      reason: "",
    }));
  // Unit values inherit through their Company-owned organization ancestry.
  // Load ancestry outside a Branch view, then expose only effective visible fields.
  const ancestors = (await store.ancestry({company}));
  for (const node of nodes) {
    let record = values.find(
      (v) => v.target === "unit" && v.objectId === node.id,
    );
    if (!record) {
      record = {
        target: "unit",
        objectId: node.id,
        expectedVersion: 0,
        version: 0,
        values: {},
        reason: "",
      };
      values.push(record);
    }
    record.inheritedFrom = {};
    let parent = ancestors.find((n) => n.id === node.id)?.parent_id;
    const seen = new Set([node.id]);
    while (parent && !seen.has(parent)) {
      seen.add(parent);
      const inherited =
        allValues.find((v) => v.target === "unit" && v.object_id === parent)
          ?.definition ?? {};
      for (const [field, value] of Object.entries(inherited))
        if (visibleFields.has(field) && !Object.hasOwn(record.values, field)) {
          record.values[field] = value;
          record.inheritedFrom[field] = parent;
        }
      parent = ancestors.find((n) => n.id === parent)?.parent_id;
    }
  }
  const candidates = (await store.candidates({company,observedNow:ports.now()}))
    .filter(
      (r) =>
        people.some((p) => p.id === r.person_id) &&
        attachments.some((a) => a.id === r.attachment_id),
    )
    .map((r) => {
      const a = attachments.find((a) => a.id === r.attachment_id)!,
        p = policies.find((p) => p.id === a.policyId);
      const status =
        r.review_status === "rejected"
          ? "rejected"
          : r.status === "active" && r.definition.validUntil <= ports.now()
            ? "expired"
            : r.status === "active" && !r.enabled
              ? "revoked"
              : r.status;
      return {
        id: r.id,
        personId: r.person_id,
        userId: r.user_id,
        attachmentId: r.attachment_id,
        policyName: p?.name ?? "Access policy",
        roleId: r.definition.roleId,
        status,
        reviewId: r.review_id,
        reviewVersion: r.review_version ?? 0,
        assignmentId: r.assignment_id,
        validUntil: r.definition.validUntil,
        explanation: `${a.target === "group" ? "Group" : "Organization"}: ${a.target === "group" ? groups.find((g) => g.id === a.targetId)?.name : nodes.find((n) => n.id === a.targetId)?.label} · ${status === "pending" ? "Independent review required" : (r.definition.blockedReason ?? status)}`,
      };
    });
  return {
    attributes,
    groups,
    policies,
    attachments,
    values,
    people,
    nodes,
    candidates,
    canManage: hasPermission(actor, "iam.manage"),
    canManageGroups:
      hasPermission(actor, "iam.manage") ||
      hasPermission(actor, "iam.directory.manage"),
    canDefine:
      actor.scopeKind === "holding" && hasPermission(actor, "iam.attributes.manage"),
    canManagePeople: hasPermission(actor, "people.manage"),
    canManagePersonAttributes: hasPermission(actor, "people.custom_attributes.write"),
    canManageOrganization: hasPermission(actor, "organization.manage"),
    canReview: hasPermission(actor, "iam.review"),
  };
}
// HR value writers do not acquire the IAM read capability. Build only the
// record/field facts needed for this command; access/group facts stay private.
async function commandSnapshot(store: UserManagementStore, actor: IamActor, change: ManagementChange): Promise<ManagementSnapshot> {
  if (change.operation !== "values" || change.input.target !== "person" || hasPermission(actor, "iam.read"))
    return managementSnapshot(store, actor);
  ports.authorize(actor, "people.custom_attributes.write");
  if (actor.platform) fail("Choose a Company workspace.", "SCOPE_FORBIDDEN", 403);
  await visiblePerson(store, actor, change.input.objectId);
  const attributes = (await store.attributes({company: actor.companyId}))
    .filter(row => row.definition.target === "person")
    .map(row => {
      const access = attributeAccess(row.definition, actor.role, actor.scopeKind);
      const companyIt = !row.definition.ownerRole && actor.role === "it_admin" && actor.scopeKind === "holding";
      return {...row.definition, id: row.id, version: row.version, canView: access.canView || companyIt, canEdit: access.canEdit || companyIt};
    }).filter(field => field.canView || field.canEdit);
  const visible = new Set(attributes.filter(field => field.canView).map(field => field.id));
  const row = await store.lockValues({actor, input: change.input});
  return {
    attributes, groups: [], policies: [], attachments: [], people: [], nodes: [], candidates: [],
    values: row ? [{target: "person", objectId: change.input.objectId, version: row.version, expectedVersion: row.version,
      values: Object.fromEntries(Object.entries(row.definition).filter(([id]) => visible.has(id))), reason: ""}] : [],
    canManage: false, canManageGroups: false, canDefine: false, canManagePeople: hasPermission(actor, "people.manage"),
    canManagePersonAttributes: true, canManageOrganization: false, canReview: false,
  };
}
async function save(
  store:UserManagementStore,
  actor: IamActor,
  table:ManagementObjectKind,
  input: { id?: string; expectedVersion: number },
  definition: unknown,
  extra?: { authorId: string; parentId: string },
) {
  const id = input.id ?? ports.id(),
    old = (await store.lockObjectVersion({table,actor,id}));
  if ((old?.version ?? 0) !== input.expectedVersion || (input.id && !old))
    fail("This item changed. Refresh and preview again.");
  const version = input.expectedVersion + 1;
  if (extra)
    (await store.replaceOwnedAttachment({table,actor,id,version,definition,extra}));
  else
    (await store.replaceObject({table,actor,id,version,definition}));
  (await store.appendObjectHistory({actor,table,id,version,definition,observedNow:ports.now()}));
  return id;
}
async function validatePolicy(
  store:UserManagementStore,
  actor: IamActor,
  policy: Pick<AccessPolicy, "grants">,
) {
  const parent = (await store.parentExpiry({actor}));
  for (const grant of policy.grants) {
    await allowedScope(store, actor, grant.scopeNodeId);
    if (
      (grant.scopeKind === "holding" && actor.scopeKind !== "holding") ||
      (grant.scopeKind !== "holding" && !grant.scopeNodeId)
    )
      fail("Choose a supported grant scope.");
    if (
      grant.scopeKind !== "holding" &&
      !(await store.grantScopeNode({actor,grant}))
    )
      fail("Grant scope and unit must match.");
    if (
      grant.validUntil <= ports.now() ||
      grant.validUntil >
        Math.min(Number(parent.valid_until), ports.now() + 90 * 86400000)
    )
      fail("Choose an expiry within 90 days and your assignment validity.");
    const role = (await store.publishedGrantRole({actor,grant}));
    if (!role || !role.scopes_json.includes(grant.scopeKind))
      fail("The role changed or does not support this scope.");
    if (
      [...role.permissions_json, ...role.grantable_json].some(
        (p: string) => !actor.iam?.grantablePermissions.includes(p),
      )
    )
      fail(
        "The role exceeds your delegated permissions.",
        "DELEGATION_EXCEEDED",
        403,
      );
    if (
      role.permissions_json.includes("company.owner") ||
      role.permissions_json.includes("iam.recovery")
    )
      fail(
        "Ownership and recovery access must use individually reviewed direct assignments.",
      );
  }
}
async function applyChange(
  store:UserManagementStore,
  actor: IamActor,
  change: ManagementChange,
) {
  ports.authorize(actor, commandPermission(change, actor));
  const snapshot = await commandSnapshot(store, actor, change);
  if (change.operation === "attribute") {
    if (actor.scopeKind !== "holding")
      fail(
        "Manage attribute definitions in Company scope.",
        "SCOPE_FORBIDDEN",
        403,
      );
    const input = change.input,
      old = snapshot.attributes.find((a) => a.id === input.id);
    for (const roleId of new Set([
      ...(input.readRoles ?? []),
      ...(input.writeRoles ?? []),
      ...(input.ownerRole ? [input.ownerRole] : []),
    ]))
      if (
        !(await store.attributeRoleActive({roleId,actor}))
      )
        fail(
          "Choose an active role for field ownership and access.",
          "ATTRIBUTE_ROLE_INVALID",
          422,
        );
    if (
      input.readRoles?.length &&
      input.writeRoles?.some((role) => !input.readRoles!.includes(role))
    )
      fail(
        "Every field editor must also be allowed to view the field.",
        "ATTRIBUTE_ACL_INVALID",
        422,
      );
    if (old && (old.type !== input.type || old.target !== input.target))
      fail("Field type and target are permanent. Create a new field instead.");
    const used = (await store.attributeRuleUse({actor,input}));
    if (
      used &&
      (input.archived ||
        (old && JSON.stringify(old.options) !== JSON.stringify(input.options)))
    )
      throw ports.error(409,"MANAGEMENT_CHANGED","Update active group rules before archiving this field or changing its choices.");
    if (old && old.options.some((o) => !input.options.includes(o)))
      fail(
        "Existing choices are retained for history. Add choices or create a replacement field.",
      );
    return save(store, actor, "attribute", input, input);
  }
  if (change.operation === "group") {
    const input = change.input;
    if (input.id) await groupRow(store, actor, input.id);
    await allowedScope(store, actor, input.scopeNodeId);
    let parent = input.parentId;
    const seen = new Set([input.id]);
    while (parent) {
      if (seen.has(parent) || seen.size > 32)
        fail("Groups cannot contain themselves or form a cycle.");
      seen.add(parent);
      const row = await groupRow(store, actor, parent);
      if (row.definition.archived || row.definition.kind !== "manual")
        fail("Choose an active manual parent group.");
      if (
        row.definition.scopeNodeId &&
        (!input.scopeNodeId ||
          !(await store.parentGroupScope({actor,input,row})).ok)
      )
        fail("A child group must stay within its parent's workspace.");
      parent = row.definition.parentId;
    }
    const children = snapshot.groups.filter((g) => g.parentId === input.id);
    for (const child of children.filter((g) => !g.archived)) {
      if (
        input.scopeNodeId &&
        (!child.scopeNodeId ||
          !(await store.childGroupScope({actor,child,input})).ok)
      )
        fail("Move child groups before narrowing this group's workspace.");
    }
    if (
      input.id &&
      (input.archived || input.kind === "automatic") &&
      children.some((g) => !g.archived)
    )
      fail(
        "Move child groups before archiving or making this group automatic.",
      );
    if (input.rule)
      for (const condition of input.rule.conditions) {
        const custom = snapshot.attributes.find(
            (a) =>
              `custom:${a.id}` === condition.field &&
              a.target === "person" &&
              !a.archived,
          ),
          builtin = personRuleFields.find((f) => f.id === condition.field),
          type = custom?.type ?? builtin?.type;
        if (!type) fail("Choose an active person field for each rule.");
        if (!["empty", "not-empty"].includes(condition.op)) {
          if (condition.value === undefined)
            fail("Each comparison needs a value.");
          if (
            ["gt", "gte", "lt", "lte"].includes(condition.op) &&
            !["number", "date"].includes(type)
          )
            fail("Ordered comparisons require a number or date field.");
          if ((condition.op === "contains") !== (type === "multi-select"))
            fail(
              "Multi-select fields use contains; other fields use comparisons.",
            );
          if (custom)
            checkedAttributeValue(
              {
                ...custom,
                type:
                  custom.type === "multi-select"
                    ? "single-select"
                    : custom.type,
              },
              condition.value,
            );
          else if (typeof condition.value !== "string")
            fail("Choose a text value for this field.");
        }
      }
    return save(store, actor, "group", input, input);
  }
  if (change.operation === "membership") {
    const input = change.input,
      row = await groupRow(store, actor, input.groupId);
    if (row.version !== input.expectedVersion)
      fail("The group changed. Refresh and preview again.");
    if (row.definition.kind !== "manual" || row.definition.archived)
      fail("Automatic groups use rules. Choose an active manual group.");
    for (const personId of input.personIds) {
      await visiblePerson(store, actor, personId);
      if (
        !(await store.memberInGroupScope({actor,personId,row})).ok
      )
        fail("A person is outside the group's workspace.");
      if (input.action === "add")
        (await store.addMember({actor,input,personId}));
      else
        (await store.removeMember({actor,input,personId}));
    }
    return save(
      store,
      actor,
      "group",
      { id: input.groupId, expectedVersion: input.expectedVersion },
      row.definition,
    );
  }
  if (change.operation === "values") {
    const input = change.input;
    if (input.target === "person")
      await visiblePerson(store, actor, input.objectId);
    else if (input.target === "group") await groupRow(store, actor, input.objectId);
    else await allowedScope(store, actor, input.objectId);
    const old = (await store.lockValues({actor,input}));
    if ((old?.version ?? 0) !== input.expectedVersion)
      fail("These values changed. Refresh first.");
    const values = { ...(old?.definition ?? {}) };
    if (input.inherit?.length) {
      if (input.target !== "unit")
        fail(
          "Only organization values support inheritance.",
          "ATTRIBUTE_INHERIT_INVALID",
          422,
        );
      for (const fieldId of input.inherit) {
        const field = snapshot.attributes.find(
          (a) => a.id === fieldId && a.target === "unit",
        );
        if (
          !field ||
          !field.canEdit
        )
          fail(
            "Your role cannot reset this field.",
            "ATTRIBUTE_WRITE_FORBIDDEN",
            403,
          );
        const inherited = (await store.inheritedValue({input,actor,fieldId}));
        checkedAttributeValue(field, inherited?.value);
        delete values[fieldId];
      }
    }
    for (const id of Object.keys(input.values))
      if (
        !snapshot.attributes.some(
          (a) =>
            a.id === id &&
            a.target === input.target &&
            !a.archived &&
            a.canEdit,
        )
      )
        fail(
          "Your role cannot edit this field or override its inherited value.",
          "ATTRIBUTE_WRITE_FORBIDDEN",
          403,
        );
    for (const field of snapshot.attributes.filter(
      (a) => a.target === input.target && !a.archived,
    )) {
      if (input.inherit?.includes(field.id)) continue;
      if (
        input.target === "unit" &&
        !Object.hasOwn(input.values, field.id) &&
        !Object.hasOwn(values, field.id)
      )
        continue;
      values[field.id] = checkedAttributeValue(
        field,
        Object.hasOwn(input.values, field.id)
          ? input.values[field.id]
          : values[field.id],
      );
    }
    const version = input.expectedVersion + 1;
    (await store.replaceValues({actor,input,version,values}));
    (await store.appendValueHistory({actor,input,version,values,observedNow:ports.now()}));
    return input.objectId;
  }
  if (change.operation === "policy") {
    if (
      change.input.id &&
      !snapshot.policies.some((p) => p.id === change.input.id)
    )
      fail("The policy is outside your workspace.", "SCOPE_FORBIDDEN", 403);
    if (!change.input.archived) await validatePolicy(store, actor, change.input);
    return save(store, actor, "policy", change.input, change.input);
  }
  const input = change.input;
  if (input.id && !snapshot.attachments.some((a) => a.id === input.id))
    fail("The attachment is outside your workspace.", "SCOPE_FORBIDDEN", 403);
  const policy = snapshot.policies.find(
    (p) => p.id === input.policyId && (!p.archived || input.archived),
  );
  if (!policy) fail("Choose an active policy within your workspace.");
  if (!input.archived) await validatePolicy(store, actor, policy);
  if (input.target === "group") {
    const group = await groupRow(store, actor, input.targetId);
    if (group.definition.archived) fail("Choose an active group.");
  } else await allowedScope(store, actor, input.targetId);
  for (const excluded of input.exclusions) {
    if (input.target === "unit") {
      await allowedScope(store, actor, excluded);
      if (
        !(await store.exclusionWithinTarget({actor,excluded,input})).ok
      )
        fail("Exclude only units beneath the target.");
    } else {
      let current: string | null = excluded,
        found = false;
      const seen = new Set<string>();
      while (current && !seen.has(current)) {
        seen.add(current);
        const row = await groupRow(store, actor, current);
        if (current === input.targetId) {
          found = true;
          break;
        }
        current = row.definition.parentId;
      }
      if (!found) fail("Exclude only groups beneath the target.");
    }
  }
  return save(store, actor, "attachment", input, input, {
    authorId: actor.userId,
    parentId: actor.assignmentId,
  });
}
async function reconcileManagement(store:UserManagementStore, companyId: string) {
  (await store.lockManagement({companyId}));
  // An explicit revocation must not become a new proposal on every reconciliation.
  (await store.rejectRevokedCandidates({companyId}));
  (await store.invalidateManagedAccess({companyId}));
  (await store.rejectReviewedCandidates({companyId}));
  const matches = (await store.eligiblePolicyMatches({companyId,observedNow:ports.now()}));
  for (const match of matches) {
    const parent = (await store.policyProposer({match}));
    if (!parent) continue;
    const proposer = {
      companyId,
      userId: match.author_id,
      assignmentId: parent.id,
      scopeKind: parent.scope_kind,
      scopeNodeId: parent.scope_node_id,
      platform: false,
      role: parent.role,
      iam: { grantablePermissions: parent.grantable_json },
    } as IamActor;
    // The legacy adapter deliberately serializes unknown JSON columns.
    const policy:AccessPolicy =
      typeof match.policy_data === "string"
        ? JSON.parse(match.policy_data)
        : match.policy_data;
    for (const [index, spec] of policy.grants.entries()) {
      // Fixed validity is never renewed by reconciliation.
      if (spec.validUntil <= ports.now()) continue;
      if (
        (await store.existingCandidate({companyId,match,index}))
      )
        continue;
      const grant = grantInput.parse({
        ...spec,
        userId: match.user_id,
        reason: `Policy: ${policy.name}`,
      });
      let blocked = "";
      try {
        await ports.validateGrant(store, proposer, grant, match.parent_id);
      } catch (error) {
        if (!(ports.ruleError(error))) throw error;
        blocked = error.message;
      }
      if (blocked) {
        (await store.recordBlockedCandidate({generatedId:ports.id(),companyId,match,index,grant,blocked,observedNow:ports.now()}));
        continue;
      }
      const id = ports.id(),
        reviewId = ports.id();
      const payload = {
        ...grant,
        candidateId: id,
        parentId: match.parent_id,
        authorizingAssignmentId: match.parent_id,
      };
      (await store.insertGrantReview({reviewId,companyId,payload,match,observedNow:ports.now(),grant}));
      (await store.insertGrantCandidate({id,companyId,match,index,grant,reviewId,observedNow:ports.now()}));
      (await store.appendGrantProposalAudit({generatedId:ports.id(),companyId,match,id,payload,observedNow:ports.now()}));
    }
  }
}
async function applyManagementReview(
  store:UserManagementStore,
  actor: IamActor,
  p: Record<string, unknown>,
) {
  if (actor.platform) fail("Company policy required.");
  const row = (await store.lockCandidate({actor,p}));
  if (
    !row ||
    row.status !== "pending" ||
    !(await store.candidateEligible({row,observedNow:ports.now()})).ok
  )
    fail("Eligibility changed. Refresh the proposal.");
  const grant = grantInput.parse(row.definition);
  await ports.validateGrant(store, actor, grant, String(p.parentId));
  const assignmentId = await ports.insertGrant(
      store,
    actor,
    grant,
    String(p.parentId),
    "policy",
    row.id,
  );
  (await store.activateCandidate({assignmentId,row}));
}
async function managementCommand(
  actor: IamActor,
  raw: unknown,
  key: string,
  preview: boolean,
  token?: string,
) {
  const change = managementChange.parse(raw);
  ports.authorize(actor, commandPermission(change, actor));
  if (!preview && ["policy", "attachment"].includes(change.operation))
    await ports.fresh(actor);
  const run = async (store:UserManagementStore) => {
    (await store.lockCommand({actor}));
    const before = await commandSnapshot(store, actor, change),
      stamp = ports.hash({change,snapshot:before});
    if (!preview && stamp !== token)
      fail("The preview changed. Review the latest impact before saving.");
    const id = await applyChange(store, actor, change);
    await reconcileManagement(store, actor.companyId);
    const after = await commandSnapshot(store, actor, change),
      changes = managementImpact(before,after);
    if (change.operation === "group")
      changes.matchingPeople =
        after.groups
          .find((g) => g.id === id)
          ?.members.map((id) => {
            const person = after.people.find((p) => p.id === id);
            const rule = change.input.rule;
            if (!person || !rule) return person?.name ?? id;
            const matched = rule.conditions
              .filter((condition) =>
                ruleMatches(
                  { mode: "all", conditions: [condition] },
                  person.facts,
                ),
              )
              .map(
                (condition) =>
                  `${after.attributes.find((a) => "custom:" + a.id === condition.field)?.label ?? personRuleFields.find((f) => f.id === condition.field)?.label ?? condition.field}: ${condition.op} ${String(condition.value ?? "")}`,
              );
            return `${person.name} — ${matched.join(" · ")}`;
          }) ?? [];
    return { id, previewToken: stamp, impact: changes };
  };
  if (!preview)
    return ports.command(
      actor,
      key,
      `user-management.${change.operation}`,
      change,
      commandPermission(change, actor),
      run,
    );
  return ports.previewTransaction(run);
}return{managementSnapshot,reconcileManagement,applyManagementReview,managementCommand,readManagement:(actor:IamActor)=>ports.readTransaction(store=>managementSnapshot(store,actor)),managementWork:()=>ports.workSession(async session=>{for(const companyId of await session.companies())await session.transaction(companyId,store=>reconcileManagement(store,companyId));})};}
