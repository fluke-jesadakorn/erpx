import {z} from 'zod';import {hasPermission} from "@/lib/identity_access/permissions";import type {IamActor} from '../../identity_access/public/iam-context';import {holdingTemplateInput,type PeopleTemplatesData} from "@/lib/people_payroll/people-templates";import {BusinessRuleError,type Clock,type IdentifierSource} from '../../shared';import {adoptTemplateInput,type TemplateRow,type SettingsRow,type AttributeRow,type TemplateRows} from '../domain/people-templates';export interface PeopleTemplateRepository{templates(companyId:string,holdingId:string | null):Promise<TemplateRows<TemplateRow>>;
publishedTemplate(id:string,holdingId:string | null):Promise<TemplateRows<TemplateRow>>;
updateTemplate(name:string,value:string,now:number,id:string):Promise<TemplateRows<TemplateRow>>;
insertTemplate(id:string,holdingId:string | null,name:string,value:string,userId:string,now:number):Promise<TemplateRows<TemplateRow>>;
adoptionTemplate(templateId:string,holdingId:string | null):Promise<TemplateRows<TemplateRow>>;
settings(companyId:string):Promise<TemplateRows<SettingsRow>>;
attributes(companyId:string):Promise<TemplateRows<AttributeRow>>;
activeRole(role:string,companyId:string):Promise<TemplateRows<Record<string,unknown>>>;
groupReferences(companyId:string,value:string):Promise<TemplateRows<Record<string,unknown>>>;
upsertAttribute(companyId:string,id:string,value:string):Promise<TemplateRows<Record<string,unknown>>>;
attributeHistory(companyId:string,id:string,value:unknown,value2:string,now:number):Promise<TemplateRows<Record<string,unknown>>>;
updateSettings(companyId:string,value:string):Promise<TemplateRows<Record<string,unknown>>>;
recordAdoption(companyId:string,id:unknown,version:unknown,value:string):Promise<TemplateRows<Record<string,unknown>>>;}export interface PeopleTemplatePorts{repository:PeopleTemplateRepository;clock:Clock;identifiers:IdentifierSource;hash(value:string):string;companyScope(actor:IamActor):void;authorize(actor:IamActor,permission:string):unknown;command<Result>(actor:IamActor,key:string,operation:string,input:unknown,permission:string,run:(repository:PeopleTemplateRepository)=>Promise<Result>):Promise<Result>;preview<Result>(run:(repository:PeopleTemplateRepository)=>Promise<Result>):Promise<Result>;}export function createPeopleTemplateApplication(ports:PeopleTemplatePorts){function scope(actor: IamActor) {
  if (actor.platform || !actor.holdingId)
    throw new BusinessRuleError(
      403,
      "HOLDING_REQUIRED",
      "Choose a Company within a Holding.",
    );
  ports.companyScope(actor);
}
async function peopleTemplatesData(
  actor: IamActor,
): Promise<PeopleTemplatesData> {
  scope(actor);
  ports.authorize(actor, "organization.manage");
  const store=ports.repository;
  const rows = (
    await store.templates(actor.companyId,actor.holdingId)
  ).rows;
  return {
    templates: rows.map((r) => ({
      id: r.id,
      name: r.name,
      version: r.version,
      definition: r.definition,
      adoptedVersion: r.adopted_version ?? null,
    })),
    canPublish: hasPermission(actor, "iam.roles.manage"),
    canAdopt: hasPermission(actor, "organization.manage"),
  };
}
async function saveHoldingTemplate(
  actor: IamActor,
  raw: unknown,
  key: string,
) {
  scope(actor);
  const input = holdingTemplateInput.parse(raw);
  return ports.command(
    actor,
    key,
    "people.template.publish",
    input,
    "iam.roles.manage",
    async (store) => {
      const id = input.id ?? ports.identifiers.next(),
        old = (
          await store.publishedTemplate(id,actor.holdingId)
        ).rows[0];
      if (Number(old?.version ?? 0) !== input.expectedVersion)
        throw new BusinessRuleError(
          409,
          "TEMPLATE_CHANGED",
          "Review the latest template before publishing.",
        );
      if (old)
        await store.updateTemplate(input.name,JSON.stringify(input.definition),ports.clock.now(),id);
      else
        await store.insertTemplate(id,actor.holdingId,input.name,JSON.stringify(input.definition),actor.userId,ports.clock.now());
      return { id, version: input.expectedVersion + 1 };
    },
  );
}
async function adoptHoldingTemplate(
  actor: IamActor,
  input: z.infer<typeof adoptTemplateInput>,
  key: string,
  preview: boolean,
) {
  scope(actor);
  ports.authorize(actor, "organization.manage");
  const run = async (store:PeopleTemplateRepository) => {
    const t = (
      await store.adoptionTemplate(input.templateId,actor.holdingId)
    ).rows[0];
    if (!t || t.version !== input.revision)
      throw new BusinessRuleError(
        409,
        "TEMPLATE_CHANGED",
        "Choose the current Holding template revision.",
      );
    const definition = holdingTemplateInput.shape.definition.parse(
      t.definition,
    );
    if (definition.attributes.length)
      ports.authorize(actor, "iam.roles.manage");
    const settings = (
      await store.settings(actor.companyId)
    ).rows[0];
    const attributes = (
      await store.attributes(actor.companyId)
    ).rows;
    const stamp = ports.hash(JSON.stringify({ template: t, settings, attributes }));
    const result = {
      previewToken: stamp,
      onboardingTasks: definition.onboardingTasks,
      offboardingTasks: definition.offboardingTasks,
      attributes: definition.attributes.map((a) => a.label),
      message:
        "Existing values and personal grants are retained. New templates affect future HR/IT work.",
    };
    if (preview) return result;
    if (input.previewToken !== stamp)
      throw new BusinessRuleError(
        409,
        "TEMPLATE_PREVIEW_CHANGED",
        "The Company configuration changed after preview.",
      );
    for (const field of definition.attributes) {
      const id = "template:" + t.id + ":" + field.key,
        old = attributes.find((a) => a.id === id);
      for (const role of new Set([
        ...(field.readRoles ?? []),
        ...(field.writeRoles ?? []),
        ...(field.ownerRole ? [field.ownerRole] : []),
      ]))
        if (
          !(
            await store.activeRole(role,actor.companyId)
          ).rowCount
        )
          throw new BusinessRuleError(
            422,
            "ATTRIBUTE_ROLE_INVALID",
            "This Company must have the field's owner and access roles before adoption.",
          );
      if (
        old &&
        (field.archived ||
          JSON.stringify(old.definition.options) !==
            JSON.stringify(field.options)) &&
        (
          await store.groupReferences(actor.companyId,"custom:" + id)
        ).rowCount
      )
        throw new BusinessRuleError(
          409,
          "TEMPLATE_FIELD_IN_USE",
          "Update active group rules before changing this template field.",
        );
      if (
        old &&
        (old.definition.type !== field.type ||
          old.definition.target !== field.target ||
          old.definition.options.some(
            (o: string) => !field.options.includes(o),
          ))
      )
        throw new BusinessRuleError(
          409,
          "TEMPLATE_FIELD_INCOMPATIBLE",
          "Retain the field type, target and existing choices when updating a shared template.",
        );
      const { key: stableKey, ...config } = field;
      void stableKey;
      await store.upsertAttribute(actor.companyId,id,JSON.stringify(config));
      await store.attributeHistory(actor.companyId,id,(old?.version ?? 0) + 1,JSON.stringify(config),ports.clock.now());
    }
    const next = {
      ...(settings?.definition ?? { itAssigneeId: null }),
      onboardingTasks: definition.onboardingTasks,
      offboardingTasks: definition.offboardingTasks,
    };
    await store.updateSettings(actor.companyId,JSON.stringify(next));
    await store.recordAdoption(actor.companyId,t.id,t.version,JSON.stringify(definition));
    return result;
  };
  if (!preview)
    return ports.command(
      actor,
      key,
      "people.template.adopt",
      input,
      "organization.manage",
      run,
    );
  return ports.preview(run);
}return {peopleTemplatesData,saveHoldingTemplate,adoptHoldingTemplate};}
