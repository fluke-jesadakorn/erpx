import {aiPresets,supportedModelProtocol,supportedModels,unsupportedModelMessage,presetForUrl} from '@/lib/ai/providers';
import {z} from 'zod';
import type {StaffActor} from '../../operations/public/staff-policy';
import {BusinessRuleError,type Clock} from '../../shared';
import {aiBaseUrl,type AiEndpoint,type AiMessage} from '../domain/transport';
import {aiConnectionSchema,aiSelectionSchema,aiDiscoverySchema,type Connection,type Revision,type Selection,type AiConnectionView,type AiProviderOverview,type AiProviderSnapshot} from '../domain/providers';
export interface AiProviderRepository<Effect>{holdingBinding():Promise<{holding_id:string}|null>;
connection(id:string,companyId:string):Promise<Connection|null>;
revision(id:string,value:number | null):Promise<Revision|null>;
guardRevisionVersion(key:string,value:"ai-version",id:string,company_id:string,expected:number):Effect;
connections(companyId:string):Promise<(Connection)[]>;
selection(companyId:string):Promise<Selection|null>;
usage(companyId:string,now:number):Promise<(AiProviderOverview["usage"][number])[]>;
prepareConnectionUpdate(name:string,rev:number,now:number,recordId:string):Effect;
prepareConnectionCreate(recordId:string,companyId:string,name:string,userId:string,now:number):Effect;
prepareRevision(recordId:string,rev:number,baseUrl:string,authMode:"bearer" | "none",credential:string | null,textModel:string,value:string | null,userId:string,now:number,providerPreset:"custom" | "opencode-zen" | "opencode-go",textProtocol:"chat-completions" | "responses" | "messages",imageProtocol:"chat-completions" | "responses" | "messages"):Effect;
prepareCapabilityTest(now:number,value:0 | 1,value2:0 | 1,error:string | null,id:string,revision:number):Effect;
prepareTestVersion(now:number,id:string):Effect;
guardTestedRevision(key:string,value:"tested-revision",id:string,draft_revision:number):Effect;
prepareActivation(now:number,id:string):Effect;
homeAiName(companyId:string):Promise<{home_ai_name:string}|null>;
guardSelectedCapability(key:string,value:string,id:string,companyId:string,version:number,value2:0 | 1):Effect;
guardSettingsVersion(key:string,value:"settings-version",companyId:string,expectedVersion:number):Effect;
prepareNameHistory(companyId:string,currentName:string):Effect;
prepareSelection(companyId:string,value:number,mode:"custom" | "deployment" | "disabled",value2:string | null,value3:string | null,imageMode:"custom" | "default" | "disabled",homeAiName:string,userId:string,now:number):Effect;
disableSelection(companyId:string):Promise<Selection|null>;

prepareDisableUses(id:string,userId:string,now:number,companyId:string):Effect;
prepareDisableConnection(value:number | null,now:number,id:string):Effect;
prepareDestroyCredentials(id:string):Effect;
currentSelection(companyId:string):Promise<Selection|null>;
prepareUsage(id:string,companyId:string,connectionId:string | null,revision:number,model:string,feature:string,status:"succeeded" | "failed",inputTokens:number,outputTokens:number,now:number,branchId:string | null,fundingSource:"unknown" | "simulation" | "byok" | "erpx"):Effect;
snapshotGuard(key:string,snapshot:AiProviderSnapshot,label:string,simulationSnapshot:boolean):Effect;}
export interface AiProviderPorts<Effect>{repository:AiProviderRepository<Effect>;clock:Clock;configuration:{production:boolean;baseUrl?:string;model?:string;apiKey?:string;keyStorageReady:boolean;simulationModel:string};simulation(holdingSlug?:string):boolean;hash(value:string):Promise<string>;sealKey(companyId:string,id:string,key:string):Promise<string>;openKey(companyId:string,id:string,sealed:string):Promise<string>;generate(endpoint:AiEndpoint,messages:AiMessage[],maxTokens?:number):Promise<{result:unknown;inputTokens:number;outputTokens:number}>;request(endpoint:AiEndpoint,path:'models'):Promise<unknown>;authority:{has(actor:StaffActor,roles:string[]):Promise<boolean>;guard(key:string,actor:StaffActor,roles:string[]):Effect};commands:{replay<Result=Record<string,unknown>>(actor:StaffActor,key:string,operation:string,id:string,input:unknown):Promise<Result|null>;commit<Result>(actor:StaffActor,key:string,operation:string,input:unknown,id:string,version:number,result:Result,effects:Effect[]):Promise<Result>};}
export function createAiProviderApplication<Effect>(ports:AiProviderPorts<Effect>){const r=ports.repository;
async function gatewayEndpoint(companyId: string, target: AiEndpoint, connectionId: string | null, holdingId?: string): Promise<AiEndpoint> {
  const holding = await r.holdingBinding();
  if (holding && holdingId && holding.holding_id !== holdingId)
    throw new BusinessRuleError(403,"AI_GATEWAY_SCOPE","The AI billing scope does not match this Holding.");
  const scopedHoldingId = holdingId ?? holding?.holding_id;
  if (!scopedHoldingId) throw new BusinessRuleError(503,"AI_GATEWAY_SCOPE","The Holding's AI billing scope is unavailable.");
  return { ...target, gateway: {
    holdingId: scopedHoldingId, companyId, branchId: null, connectionId,
    fundingSource: connectionId ? "byok" : "erpx",
    feature: "", requestId: "",
  } };
}
const roles = ["holding_owner", "it_admin"];
async function requireAiAdmin(actor: StaffActor) {
  if (!(await ports.authority.has(actor, roles)))
    throw new BusinessRuleError(
      403,
      "AI_ADMIN_REQUIRED",
      "A company owner or IT administrator is required.",
    );
}
const guard = (key: string, actor: StaffActor) =>
  ports.authority.guard(key, actor, roles);
async function connection(companyId: string, id: string) {
  const row = await r.connection(id,companyId);
  if (!row)
    throw new BusinessRuleError(
      404,
      "AI_CONNECTION_NOT_FOUND",
      "AI connection not found in this company.",
    );
  return row;
}
async function revision(row: Connection, active = false) {
  const result = await r.revision(row.id,active ? row.active_revision : row.draft_revision);
  if (!result)
    throw new BusinessRuleError(
      409,
      "AI_REVISION_UNAVAILABLE",
      "This connection has no usable configuration revision.",
    );
  return result;
}
const versionGuard = (
  key: string,
  row: Connection,
  expected: number,
) =>
  r.guardRevisionVersion(key,"ai-version",row.id,row.company_id,expected);
async function aiProviderOverview(
  actor: StaffActor,
  ): Promise<AiProviderOverview> {
  await requireAiAdmin(actor);
  const rows = (await r.connections(actor.companyId));
  const connections: AiConnectionView[] = [];
  for (const row of rows) {
    const rev = await revision(row);
    const { credential_json, ...safe } = rev;
    const active = row.active_revision ? await revision(row, true) : null;
    connections.push({
      ...row,
      ...safe,
      hasKey: !!credential_json,
      active: active
        ? {
            base_url: active.base_url,
            text_protocol: active.text_protocol,
            image_protocol: active.image_protocol,
            text_model: active.text_model,
            image_model: active.image_model,
          }
        : null,
    });
  }
  const selection = (await r.selection(actor.companyId)) ?? {
    company_id: actor.companyId,
    version: 0,
    mode: ports.configuration.production ? "disabled" : "deployment",
    text_connection_id: null,
    image_connection_id: null,
    image_mode: "default",
    home_ai_name: "AI",
  };
  const usage = (await r.usage(actor.companyId,ports.clock.now() - 30 * 86400000));
  return {
    connections,
    selection,
    deploymentAvailable: !!(ports.configuration.baseUrl && ports.configuration.model && ports.configuration.apiKey),
    keyStorageReady: !!ports.configuration.keyStorageReady,
    usage,
  };
}
async function saveAiConnection(
  actor: StaffActor,
  id: string | null,
  input: z.infer<typeof aiConnectionSchema>,
  key: string,
) {
  await requireAiAdmin(actor);
  const recordId = id ?? `ai-${key}`,
    operation = "ai.connection.save";
  const prior = await ports.commands.replay<{id:string;version:number}>(actor, key, operation, recordId, input);
  if (prior) return prior;
  const baseUrl = aiBaseUrl(input.baseUrl),
    row = id ? await connection(actor.companyId, id) : null,
    previous = row ? await revision(row) : null;
  if (input.providerPreset !== "custom" && baseUrl !== aiPresets[input.providerPreset].baseUrl)
    throw new BusinessRuleError(422, "AI_PRESET_URL", "Use the preset URL or choose Custom API.");
  let credential = previous?.credential_json ?? null;
  if (previous && baseUrl !== previous.base_url && credential && input.authMode === "bearer" && !input.removeKey && !input.apiKey?.trim())
    throw new BusinessRuleError(422, "AI_KEY_REQUIRED", "Enter a new API key when changing the provider URL.");
  const textProtocol = supportedModelProtocol(baseUrl, input.textModel);
  const imageProtocol = input.imageModel ? supportedModelProtocol(baseUrl, input.imageModel) : "chat-completions";
  if (!textProtocol || !imageProtocol)
    throw new BusinessRuleError(422, "AI_MODEL_UNSUPPORTED", unsupportedModelMessage);
  if (input.removeKey || input.authMode === "none") credential = null;
  if (input.apiKey?.trim() && input.authMode === "bearer")
    credential = await ports.sealKey(actor.companyId,
      recordId,
      input.apiKey.trim(),
    );
  const rev = (row?.draft_revision ?? 0) + 1,
    now = ports.clock.now(),
    version = (row?.version ?? 0) + 1;
  return ports.commands.commit(actor,
    key,
    operation,
    input,
    recordId,
    version,
    { id: recordId, version },
    [
      guard(key, actor),
      ...(row ? [versionGuard(key, row, input.expectedVersion)] : []),
      row
        ? r.prepareConnectionUpdate(input.name,rev,now,recordId)
        : r.prepareConnectionCreate(recordId,actor.companyId,input.name,actor.userId,now),
      r.prepareRevision(recordId,rev,baseUrl,input.authMode,credential,input.textModel,input.imageModel || null,actor.userId,now,input.providerPreset,textProtocol,imageProtocol),
    ],
  );
}
async function endpoint(
  companyId: string,
  row: Connection,
  rev: Revision,
  kind: "text" | "image",
): Promise<AiEndpoint> {
  if (rev.auth_mode === "bearer" && !rev.credential_json)
    throw new BusinessRuleError(
      422,
      "AI_KEY_REQUIRED",
      "Enter an API key before testing this Bearer connection.",
    );
  return {
    baseUrl: rev.base_url,
    protocol: kind === "image" ? rev.image_protocol : rev.text_protocol,
    apiKey:
      rev.auth_mode === "bearer" && rev.credential_json
        ? await ports.openKey(companyId, row.id, rev.credential_json)
        : null,
    model:
      kind === "image" ? (rev.image_model ?? rev.text_model) : rev.text_model,
  };
}
async function testAiConnection(
  actor: StaffActor,
  id: string,
  expectedVersion: number,
  key: string,
  
) {
  await requireAiAdmin(actor);
  const input = { expectedVersion },
    operation = "ai.connection.test",
    prior = await ports.commands.replay(actor, key, operation, id, input);
  if (prior) return prior;
  const row = await connection(actor.companyId, id),
    rev = await revision(row);
  if (row.version !== expectedVersion)
    throw new BusinessRuleError(
      409,
      "AI_VERSION_CONFLICT",
      "The connection changed. Refresh before testing.",
    );
  let textVerified = false,
    imageVerified = false,
    error: string | null = null;
  try {
    const result = await ports.generate(
      { ...await endpoint(actor.companyId, row, rev, "text"), sessionId: await ports.hash(`ai-test:${actor.companyId}:${id}:${rev.revision}`) },
      [
        {
          role: "system",
          content: "Return a JSON object with ok equal to true.",
        },
        {
          role: "user",
          content: 'Synthetic connection test. Return {"ok":true}.',
        },
      ],
    );
    textVerified = z
      .object({ ok: z.literal(true) })
      .safeParse(result.result).success;
    if (!textVerified)
      throw new BusinessRuleError(
        503,
        "AI_JSON_UNSUPPORTED",
        "The text model did not return the expected JSON test result.",
      );
    if (rev.image_model) {
      const vision = await ports.generate(
        { ...await endpoint(actor.companyId, row, rev, "image"), sessionId: await ports.hash(`ai-test:${actor.companyId}:${id}:${rev.revision}`) },
        [
          {
            role: "system",
            content:
              "Identify the solid color of the supplied synthetic image. Return JSON with one string field named color. Use only the actual image. Use one lowercase English color name without descriptions or color codes.",
          },
          {
            role: "user",
            content: [
              { type: "text", text: "What color is this image?" },
              {
                type: "image_url",
                image_url: {
                  url: "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAQAAAAEACAIAAADTED8xAAAACXBIWXMAAAPoAAAD6AG1e1JrAAAFG0lEQVR4nO3bwQ0AIQzEwPTftKmBVx4ZiQKQZXQH2Z1mLAS6qsH+DiwEcgBIgEC+ACRAIL9AJEAgdwASIJBLMAkQyCsQCRDIMygJEMgcgAQIZBBGAgQyCSYBAolCkACBZIFIgEDCcCRAIGlQEiCQODQJEEgfgAQIpBBDAgTSCCMBAqlEkgCBdIJJkEK2UjwJun0M9ndgIZADQAIE8gUgAQL5BSIBArkDkACBXIJJgEBegUiAQJ5BSYBA5gAkQCCDMBIgkEkwCRBIFIIECCQLRAIEEoYjAQJJg5IAgcShSYBA+gAkQCCFGBIgkEYYCRBIJZIECKQTTIIUspXiSdDtY7C/AwuBHAASIJAvAAkQyC8QCRDIHYAECOQSTAIE8gpEAgTyDEoCBDIHIAECGYSRAIFMgkmAQKIQJEAgWSASIJAwHAkQSBqUBAgkDk0CBNIHIAECKcSQAIE0wkiAQCqRJEAgnWASpJCtFE+Cbh+D/R1YCOQAkACBfAFIgEB+gUiAQO4AJEAgl2ASIJBXIBIgkGdQEiCQOQAJEMggjAQIZBJMAgQShSABAskCkQCBhOFIgEDSoCRAIHFoEiCQPgAJEEghhgQIaISRAIFRiSQBAqMTTAIElOJJMNePwf4OLARyAEiAQL4AJEAgv0AkQCB3ABIgkEswCRDIKxAJEMgzKAkQyByABAhkEEYCBDIJJgECiUKQAIFkgUiAQMJwJEAgaVASIJA4NAkQSB+ABAikEEMCBNIIIwECqUSSAIF0gkmQQrZSPAm6fQz2d2AhkANAAgTyBSABAvkFIgECuQOQAIFcgkmAQF6BSIBAnkFJgEDmACRAIIMwEiCQSTAJEEgUggQIJAtEAgQShiMBAkmDkgCBxKFJgED6ACRAIIUYEiCQRhgJEEglkgQIpBNMghSyleJJ0O1jsL8DC4EcABIgkC8ACRDILxAJEMgdgAQI5BJMAgTyCkQCBPIMSgIEMgcgAQIZhJEAgUyCSYBAohAkQCBZIBIgkDAcCRBIGpQECCQOTQIE0gcgAQIpxJAAgTTCSIBAKpEkQCCdYBKkkK0UT4JuH4P9HVgI5ACQAIF8AUiAQH6BSIBA7gAkQCCXYBIgkFcgEiCQZ1ASIJA5AAkQyCCMBAhkEkwCBBKFIAECyQKRAIGE4UiAQNKgJEAgcWgSIJA+AAkQSCGGBAikEUYCBFKJJAEC6QSTIIVspXgSdPsY7O/AQiAHgAQI5AtAAgTyC0QCBHIHIAECuQSTAIG8ApEAgTyDkgCBzAFIgEAGYSRAIJNgEiCQKAQJEEgWiAQIJAxHAgSSBiUBAolDkwCB9AFIgEAKMSRAII0wEiCQSiQJEEgnmAQpZCvFk6Dbx2B/BxYCOQAkQCBfABIgkF8gEiCQOwAJEMglmAQI5BWIBAjkGZQECGQOQAIEMggjAQKZBJMAgUQhSIBAskAkQCBhOBIgIA1KAgRGHJoECIw+AAkQGIUYEiAwGmEkQGBUIkmAwOgEkwABpXgSzPVjsL8DC4EcABIgkC8ACRDILxAJEMgdgAQI5BJMAgTyCkQCBPIMSgIEMgcgAQIZhJEAgUyCSYBAohAkQCBZIBIgkDAcCRBIGpQECCQOTQIE0gcgAQIpxJAAgTTCSIBAKpEkQCCdYBKkkK0UT4JuH4P9HVgI5ACQAIF8AUiAQH6BSIBA7gAkQCCXYBIgkFcgEiCQZ1ASIJA5AAkQyCCMBAhkEkwCBBKFIAECyQKRAIH+j8EDWUwO8hZP+cEAAAAASUVORK5CYII=",
                },
              },
            ],
          },
        ],
      );
      imageVerified = z
        .object({ color: z.string().trim().toLowerCase().pipe(z.literal("red")) })
        .safeParse(vision.result).success;
      if (!imageVerified)
        error = "The image model did not pass the synthetic image test.";
    }
  } catch (e) {
    error =
      e instanceof BusinessRuleError
        ? e.message
        : "The connection test failed. Check the endpoint and model.";
  }
  const now = ports.clock.now();
  return ports.commands.commit(actor,
    key,
    operation,
    input,
    id,
    expectedVersion + 1,
    { id, version: expectedVersion + 1, textVerified, imageVerified, error },
    [
      guard(key, actor),
      versionGuard(key, row, expectedVersion),
      r.prepareCapabilityTest(now,textVerified ? 1 : 0,imageVerified ? 1 : 0,error,id,rev.revision),
      r.prepareTestVersion(now,id),
    ],
  );
}
async function discoverAiModelsInput(
  actor: StaffActor, input: z.infer<typeof aiDiscoverySchema>,
) {
  await requireAiAdmin(actor);
  const baseUrl = aiBaseUrl(input.baseUrl);
  const row = input.connectionId ? await connection(actor.companyId, input.connectionId) : null;
  const rev = row ? await revision(row) : null;
  let apiKey = input.authMode === "bearer" ? input.apiKey?.trim() || null : null;
  if (!apiKey && input.authMode === "bearer" && !input.removeKey && rev?.credential_json) {
    if (baseUrl !== rev.base_url)
      throw new BusinessRuleError(422, "AI_KEY_REQUIRED", "Enter a new API key when changing the provider URL.");
    apiKey = await ports.openKey(actor.companyId, row!.id, rev.credential_json);
  }
  // OpenCode publishes its catalog without authentication; inference still needs a key.
  if (!apiKey && input.authMode === "bearer" && presetForUrl(baseUrl) === "custom")
    throw new BusinessRuleError(422, "AI_KEY_REQUIRED", "Enter an API key to fetch models.");
  const raw = await ports.request({ baseUrl, apiKey, model: "" }, "models");
  const parsed = z.object({ data: z.array(z.object({ id: z.string().trim().min(1).max(200) })) }).safeParse(raw);
  if (!parsed.success)
    throw new BusinessRuleError(503, "AI_INVALID_RESPONSE", "The provider returned an invalid model list.");
  const allModels = [...new Set(parsed.data.data.map(m => m.id))];
  return { models: supportedModels(baseUrl, allModels), totalModels: allModels.length, fetchedAt: new Date(ports.clock.now()).toISOString(), message: null };
}
async function discoverAiModels(actor: StaffActor, id: string) {
  await requireAiAdmin(actor);
  const row = await connection(actor.companyId, id), rev = await revision(row);
  return discoverAiModelsInput(actor,{ baseUrl: rev.base_url, authMode: rev.auth_mode, connectionId: id, removeKey: false });
}
async function activateAiConnection(
  actor: StaffActor,
  id: string,
  expectedVersion: number,
  key: string,
) {
  await requireAiAdmin(actor);
  const input = { expectedVersion },
    operation = "ai.connection.activate",
    prior = await ports.commands.replay(actor, key, operation, id, input);
  if (prior) return prior;
  const row = await connection(actor.companyId, id),
    rev = await revision(row);
  if (!rev.text_verified)
    throw new BusinessRuleError(
      422,
      "AI_TEST_REQUIRED",
      "Test this revision successfully before activation.",
    );
  return ports.commands.commit(actor,
    key,
    operation,
    input,
    id,
    expectedVersion + 1,
    { id, version: expectedVersion + 1 },
    [
      guard(key, actor),
      versionGuard(key, row, expectedVersion),
      r.guardTestedRevision(key,"tested-revision",id,row.draft_revision),
      r.prepareActivation(ports.clock.now(),id),
    ],
  );
}
async function selectAiProviders(
  actor: StaffActor,
  input: z.infer<typeof aiSelectionSchema>,
  key: string,
) {
  await requireAiAdmin(actor);
  const operation = "ai.settings.save",
    prior = await ports.commands.replay(actor,
      key,
      operation,
      actor.companyId,
      input,
    );
  if (prior) return prior;
  const currentName = (await r.homeAiName(actor.companyId))?.home_ai_name ?? "AI";
  const homeAiName=input.homeAiName??currentName;
  if (homeAiName !== currentName && !(await ports.authority.has(actor, ["holding_owner"])))
    throw new BusinessRuleError(403, "AI_OWNER_REQUIRED", "Only the company owner can rename the Home AI.");
  const checks: Effect[] = [];
  for (const [id, kind] of (input.mode === "custom"
    ? [
        [input.textConnectionId, "text"],
        ...(input.imageMode === "custom"
          ? [[input.imageConnectionId, "image"]]
          : []),
      ]
    : []) as [string | null, "text" | "image"][]) {
    if (!id)
      throw new BusinessRuleError(
        422,
        "AI_CONNECTION_REQUIRED",
        "Choose an active connection.",
      );
    const row = await connection(actor.companyId, id),
      rev = await revision(row, true);
    if (
      row.disabled ||
      !rev.text_verified ||
      (kind === "image" && !rev.image_verified)
    )
      throw new BusinessRuleError(
        422,
        "AI_CAPABILITY_REQUIRED",
        "Choose an active connection with the required tested capability.",
      );
    checks.push(
      r.guardSelectedCapability(key,`selection-${kind}`,id,actor.companyId,row.version,kind === "image" ? 1 : 0),
    );
  }
  return ports.commands.commit(actor,
    key,
    operation,
    input,
    actor.companyId,
    input.expectedVersion + 1,
    { version: input.expectedVersion + 1 },
    [
      guard(key, actor),
      ...(homeAiName!==currentName?[ports.authority.guard(key,actor,["holding_owner"])]:[]),
      r.guardSettingsVersion(key,"settings-version",actor.companyId,input.expectedVersion),
      ...checks,
      ...(homeAiName!==currentName?[r.prepareNameHistory(actor.companyId,currentName)]:[]),
      r.prepareSelection(actor.companyId,input.expectedVersion + 1,input.mode,input.mode === "custom" ? input.textConnectionId : null,input.mode === "custom" && input.imageMode === "custom"
            ? input.imageConnectionId
            : null,input.imageMode,homeAiName,actor.userId,ports.clock.now()),
    ],
  );
}
async function disableAiConnection(
  actor: StaffActor,
  id: string,
  input: { expectedVersion: number; disableUses: boolean; remove: boolean },
  key: string,
) {
  await requireAiAdmin(actor);
  const operation = input.remove
      ? "ai.connection.delete"
      : "ai.connection.disable",
    prior = await ports.commands.replay(actor, key, operation, id, input);
  if (prior) return prior;
  const row = await connection(actor.companyId, id),
    settings = await r.disableSelection(actor.companyId);
  const used =
    settings?.mode === "custom" &&
    (settings.text_connection_id === id ||
      (settings.image_mode === "custom" &&
        settings.image_connection_id === id));
  if (used && !input.disableUses)
    throw new BusinessRuleError(
      409,
      "AI_CONNECTION_IN_USE",
      "Choose a replacement first, or explicitly disable AI using this connection.",
    );
  const now = ports.clock.now();
  return ports.commands.commit(actor,
    key,
    operation,
    input,
    id,
    input.expectedVersion + 1,
    { id, version: input.expectedVersion + 1 },
    [
      guard(key, actor),
      versionGuard(key, row, input.expectedVersion),
      r.guardSettingsVersion(key,"settings-version",actor.companyId,settings?.version ?? 0),
      ...(used
        ? [
            r.prepareDisableUses(id,actor.userId,now,actor.companyId),
          ]
        : []),
      r.prepareDisableConnection(input.remove ? now : null,now,id),
      ...(input.remove
        ? [
            r.prepareDestroyCredentials(id),
          ]
        : []),
    ],
  );
}
async function resolveAiProvider(
  companyId: string,
  kind: "text" | "image",
  holdingSlug?: string,
  holdingId?: string,
) {
  const settings = await r.currentSelection(companyId);
  if (ports.simulation(holdingSlug)) {
    if (
      settings?.mode === "disabled" ||
      (kind === "image" && settings?.image_mode === "disabled")
    )
      throw new BusinessRuleError(
        503,
        "AI_DISABLED",
        "AI is disabled for this capability. Continue manually.",
      );
    return {
      endpoint: {
        baseUrl: "simulation://thai-hotel-v1",
        model: ports.configuration.simulationModel,
        apiKey: null,
      },
      snapshot: {
        companyId,
        connectionId: null,
        revision: 1,
        settingsVersion: settings?.version ?? 0,
        model: ports.configuration.simulationModel,
        kind,
        deploymentDigest: `simulation:${ports.configuration.simulationModel}`,
      } satisfies AiProviderSnapshot,
    };
  }
  if (ports.configuration.production && (!settings || settings.mode === "disabled"))
    throw new BusinessRuleError(
      503,
      "AI_PILOT_OPT_IN_REQUIRED",
      "AI is not enabled for this company. Continue manually or ask an administrator to enable a provider.",
    );
  if (
    settings?.mode === "disabled" ||
    (kind === "image" && settings?.image_mode === "disabled")
  )
    throw new BusinessRuleError(
      503,
      "AI_DISABLED",
      "AI is disabled for this capability. Continue manually.",
    );
  if (!settings || settings.mode === "deployment") {
    if (!ports.configuration.baseUrl || !ports.configuration.model || !ports.configuration.apiKey)
      throw new BusinessRuleError(
        503,
        "AI_UNAVAILABLE",
        "No AI connection is configured. Continue manually or ask an owner or IT administrator to configure AI providers.",
      );
    if (kind === "image")
      throw new BusinessRuleError(
        503,
        "AI_IMAGE_TEST_REQUIRED",
        "Configure and test an image model in AI providers to read documents.",
      );
    return {
      endpoint: await gatewayEndpoint(companyId,{
        baseUrl: ports.configuration.baseUrl,
        model: ports.configuration.model,
        apiKey: ports.configuration.apiKey,
      },null,holdingId),
      snapshot: {
        companyId,
        connectionId: null,
        revision: 0,
        settingsVersion: settings?.version ?? 0,
        model: ports.configuration.model,
        kind,
        deploymentDigest: await ports.hash(
          `${ports.configuration.baseUrl}:${ports.configuration.model}:${ports.configuration.apiKey}`,
        ),
      } satisfies AiProviderSnapshot,
    };
  }
  const id =
    kind === "image" && settings.image_mode === "custom"
      ? settings.image_connection_id
      : settings.text_connection_id;
  if (!id)
    throw new BusinessRuleError(
      503,
      "AI_UNAVAILABLE",
      "Choose an AI connection in Administration.",
    );
  const row = await connection(companyId, id),
    rev = await revision(row, true);
  if (
    row.disabled ||
    !rev.text_verified ||
    (kind === "image" && !rev.image_verified)
  )
    throw new BusinessRuleError(
      503,
      "AI_CAPABILITY_UNAVAILABLE",
      "The selected connection is disabled or has not passed the required capability test.",
    );
  const target = await endpoint(companyId, row, rev, kind);
  return {
    endpoint: await gatewayEndpoint(companyId,target,id,holdingId),
    snapshot: {
      companyId,
      connectionId: id,
      revision: rev.revision,
      settingsVersion: settings.version,
      model: target.model,
      kind,
    } satisfies AiProviderSnapshot,
  };
}
function aiSnapshotGuard(key:string,snapshot:AiProviderSnapshot,label='ai-provider-revision'){return r.snapshotGuard(key,snapshot,label,snapshot.deploymentDigest?.startsWith('simulation:')??false);}
function aiUsageStatement(
  id: string,
  snapshot: AiProviderSnapshot,
  feature: string,
  status: "succeeded" | "failed",
  inputTokens = 0,
  outputTokens = 0,
  branchId: string | null = null,
) {
  const fundingSource = snapshot.deploymentDigest?.startsWith("simulation:")
    ? "simulation"
    : snapshot.connectionId
      ? "byok"
      : snapshot.deploymentDigest
        ? "erpx"
        : "unknown";
  return r.prepareUsage(id,snapshot.companyId,snapshot.connectionId,snapshot.revision,snapshot.model,feature,status,inputTokens,outputTokens,ports.clock.now(),branchId,fundingSource);
}
return {requireAiAdmin,aiProviderOverview,saveAiConnection,testAiConnection,discoverAiModelsInput,discoverAiModels,activateAiConnection,selectAiProviders,disableAiConnection,resolveAiProvider,aiSnapshotGuard,aiUsageStatement};}
