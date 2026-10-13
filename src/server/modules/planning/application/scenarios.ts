import {z} from 'zod';import type {WorkContext} from '../../identity_access/public/authorization';import type {Row} from '../../commerce/public/commerce';import {hasPermission} from "@/lib/identity_access/permissions";import {hasModuleAccess} from '@/data/module-access';import {BusinessRuleError,type Clock} from '../../shared';import type {PlanningContext} from '../domain/capacity';import type {calculateProductionCapacity} from '../public/capacity';import {planSchema,recommendationSchema,reason,version,q,type PlanningScenarioResult,type PlanningExecutiveEvidenceMetric} from '../domain/scenarios';export interface PlanningScenarioRepository<Effect>{specification(value:(string | number)[],v:number|undefined):Promise<Row|null>;
specifications(companyId:string):Promise<{results:Row[]}>;
scenarios(companyId:string,branchId:string | null,branchId2:string | null):Promise<{results:Row[]}>;
recommendations(companyId:string,branchId:string | null,branchId2:string | null):Promise<{results:Row[]}>;
orders(companyId:string,branchId:string | null,branchId2:string | null):Promise<{results:Row[]}>;
products(companyId:string):Promise<{results:Row[]}>;
warehouses(companyId:string,branchId:string | null,branchId2:string | null):Promise<{results:Row[]}>;
resources(companyId:string,branchId:string | null,branchId2:string | null):Promise<{results:Row[]}>;
calendar(companyId:string,branchId:string | null,branchId2:string | null):Promise<{results:Row[]}>;
accountingProfiles(companyId:string):Promise<{results:Row[]}>;
demandSources(companyId:string,branchId:string | null,branchId2:string | null):Promise<{results:Row[]}>;
accounts(companyId:string,companyId2:string):Promise<{results:Row[]}>;
executiveShortage(recordId:string,companyId:string,branchId:string | null,branchId2:string | null,from:string,to:string):Promise<Row|null>;
executiveRecommendation(recordId:string,companyId:string,branchId:string | null,branchId2:string | null,from:string,to:string,eligibilityKind:"proposed"|"capacity"):Promise<Row|null>;
activeWarehouse(warehouseId:string,companyId:string,branchId:string | null,branchId2:string | null):Promise<Row|null>;
demandSource(salesOrderId:string,companyId:string,warehouseId:string,value:number,output_product_id:string | number | null):Promise<Row|null>;
specificationLines(specificationId:string):Promise<{results:Row[]}>;
availableStock(warehouseId:string,product_id:string | number | null):Promise<{ available: number; version: number }|null>;
incomingSupply(companyId:string,warehouseId:string,dueDate:string,product_id:string | number | null):Promise<{results:Row[]}>;
comparison(comparisonScenarioId:string,companyId:string,branch_id:string | number | null,specificationId:string):Promise<Row|null>;
guardComparison(key:string,value:"comparison",comparisonScenarioId:string,companyId:string,value2:string,specificationId:string,value3:number):Effect;
guardScenarioSpecification(key:string,value:"specification-source",specificationId:string,companyId:string,specificationVersion:number):Effect;
prepareScenario(id:string,companyId:string,branch_id:string | number | null,warehouseId:string,specificationId:string,specificationVersion:number,demand:number,dueDate:string,capacityMinutes:number,value:string,userId:string,now:number,now2:number,value2:string):Effect;
prepareRecommendation(recommendationId:string,id:string,value:1,demand:number,requiredMinutes:number,feasible:string,userId:string,now:number,now2:number):Effect;
prepareCalculateEvent(value:string,id:string,userId:string,value2:"Calculated from confirmed Sales demand and captured supply/capacity" | "Calculated from declared manual demand and captured supply/capacity",now:number):Effect;
scenario(id:string,companyId:string,branchId:string | null,branchId2:string | null):Promise<Row|null>;
lockScenarioRecommendations(id:string):Effect;
guardUnreleasedScenario(key:string,value:"scenario",id:string,companyId:string,expectedVersion:number):Effect;
prepareScenarioCancellation(now:number,id:string):Effect;
prepareRecommendationCancellation(userId:string,now:number,id:string):Effect;
prepareCancelEvent(value:string,id:string,value2:number,userId:string,reason:string,now:number):Effect;
scopedRecommendation(id:string,companyId:string,branchId:string | null,branchId2:string | null):Promise<Row|null>;
lockRecommendation(id:string):Effect;
guardIndependentRecommendation(key:string,value:"recommendation",id:string,expectedVersion:number,userId:string):Effect;
prepareRecommendationReview(status:"approved" | "rejected",userId:string,now:number,id:string):Effect;
guardRecommendationSpecification(key:string,value:"specification-source",value2:string,companyId:string,value3:number):Effect;
guardCapacityReservation(key:string,value:string,resourceId:string,workDate:string,minutes:number,availableMinutes:number):Effect;
prepareCapacityReservation(value:string,scenario_id:string | number | null,resourceId:string,workDate:string,minutes:number,userId:string,now:number):Effect;
prepareManufacturingOrder(orderId:string,companyId:string,branch_id:string | number | null,warehouse_id:string | number | null,id:string,specification_id:string | number | null,specification_version:string | number | null,recommended_quantity_milli:string | number | null,userId:string,now:number,now2:number):Effect;
prepareScenarioRelease(now:number,scenario_id:string | number | null):Effect;
prepareReleaseEvent(value:string,userId:string,reason:string,now:number,scenario_id:string | number | null):Effect;
prepareManufacturingReleaseEvent(value:string,orderId:string,userId:string,reason:string,now:number):Effect;}export interface OwnerCommands<Effect>{replay<Result=Record<string,unknown>>(actor:WorkContext,key:string,operation:string,id:string,input:unknown):Promise<Result|null>;commit<Result>(actor:WorkContext,key:string,operation:string,input:unknown,id:string,version:number,result:Result,effects:Effect[],scope?:Pick<WorkContext,'companyId'|'branchId'|'departmentId'>):Promise<Result>;}export interface PlanningScenarioPorts<Effect>{repository:PlanningScenarioRepository<Effect>;commands:OwnerCommands<Effect>;clock:Clock;requirePermission(actor:WorkContext,permission:string):WorkContext;calculateCapacity(actor:WorkContext,input:Parameters<typeof calculateProductionCapacity>[2]):ReturnType<typeof calculateProductionCapacity>;contextGuards(actor:WorkContext,key:string,context:PlanningContext):Effect[];}export function createPlanningScenarioApplication<Effect>(ports:PlanningScenarioPorts<Effect>){const r=ports.repository;function access(
  a: WorkContext,
  module: "planning" | "manufacturing",
  write = false,
) {
  ports.requirePermission(a, write ? "records.write" : "records.read");
  if (!hasModuleAccess(a.modules, module))
    throw new BusinessRuleError(
      403,
      "MODULE_FORBIDDEN",
      `Your current role cannot access ${module}.`,
    );
}
async function specRecord(
  
  a: WorkContext,
  id: string,
  v?: number,
) {
  return r.specification((v ? [id, a.companyId, v] : [id, a.companyId]),v);
}
async function productionWorkspace(
  a: WorkContext,
  module: "planning" | "manufacturing",
) {
  access(a, module);
  const 
    [specs, plans, recommendations, orders, products, warehouses, resources, calendar, accountingProfiles, demandSources, accounts] =
      await Promise.all([
        r.specifications(a.companyId),
        r.scenarios(a.companyId,a.branchId,a.branchId),
        r.recommendations(a.companyId,a.branchId,a.branchId),
        r.orders(a.companyId,a.branchId,a.branchId),
        r.products(a.companyId),
        r.warehouses(a.companyId,a.branchId,a.branchId),
        r.resources(a.companyId,a.branchId,a.branchId),
        r.calendar(a.companyId,a.branchId,a.branchId),
        r.accountingProfiles(a.companyId),
        r.demandSources(a.companyId,a.branchId,a.branchId),
        hasPermission(a, "finance.read") ? r.accounts(a.companyId,a.companyId) : Promise.resolve({ results: [] as Row[] }),
      ]);
  return {
    specifications: specs.results,
    plans: plans.results,
    recommendations: recommendations.results,
    orders: orders.results,
    products: products.results,
    warehouses: warehouses.results,
    resources: resources.results,
    calendar: calendar.results,
    accountingProfiles: accountingProfiles.results,
    demandSources: demandSources.results,
    accounts: accounts.results,
    canConfigureAccounting: !a.branchId && !a.departmentId && hasPermission(a, "finance.write"),
    canReviewAccounting: !a.branchId && !a.departmentId && hasPermission(a, "finance.approve"),
    canManageResources: !a.departmentId && hasPermission(a, "records.write") && hasPermission(a, "records.review"),
  };
}
async function planningExecutiveEvidenceRow(a:WorkContext,metric:PlanningExecutiveEvidenceMetric,recordId:string,from:string,to:string){access(a,'planning');return metric==='planning-shortage-scenarios'?r.executiveShortage(recordId,a.companyId,a.branchId,a.branchId,from,to):r.executiveRecommendation(recordId,a.companyId,a.branchId,a.branchId,from,to,metric==='proposed-planning-recommendations'?'proposed':'capacity');}
async function createPlanningScenario(a: WorkContext, raw: unknown, key: string): Promise<PlanningScenarioResult> {
  access(a, "planning", true);
  const i = planSchema.parse(raw), id = `plan-${key}`,
    recommendationId = `plan-recommendation-${key}`, op = "planning.scenario.create",
    prior = await ports.commands.replay<PlanningScenarioResult>( a, key, op, id, i);
  if (prior) return prior;
  const spec = await specRecord( a, i.specificationId, i.specificationVersion);
  if (!spec || spec.status !== "approved")
    throw new BusinessRuleError(409, "PLANNING_SOURCE_CHANGED", "Choose a current approved specification.");
  const warehouse = await r.activeWarehouse(i.warehouseId,a.companyId,a.branchId,a.branchId);
  if (!warehouse) throw new BusinessRuleError(404, "WAREHOUSE_NOT_FOUND", "Warehouse not found in scope.");
  const context: PlanningContext = { incomingSupply: [], stock: [], startDate: i.startDate ?? i.dueDate, comparisonScenarioId: i.comparisonScenarioId ?? null };
  let demand = i.demandQuantity ? q(i.demandQuantity) : 0;
  if (i.salesOrderId) {
    const source = await r.demandSource(i.salesOrderId,a.companyId,i.warehouseId,i.salesOrderVersion!,spec.output_product_id);
    if (!source) throw new BusinessRuleError(409, "PLANNING_DEMAND_SOURCE_CHANGED", "Choose the current confirmed Sales order for this output and warehouse.");
    demand = Number(source.remaining_milli);
    context.demandSource = { id: String(source.id), version: Number(source.version), quantityMilli: demand };
  }
  if (!Number.isSafeInteger(demand) || demand <= 0) throw new BusinessRuleError(422, "PLANNING_DEMAND_REQUIRED", "Demand must be a positive exact quantity.");
  const factor = demand / Number(spec.batch_quantity_milli),
    lines = (await r.specificationLines(i.specificationId)).results,
    shortages: Array<{ productId: string; requiredMilli: number; availableMilli: number; incomingMilli: number; shortageMilli: number }> = [];
  for (const line of lines) {
    const required = Math.ceil(Number(line.quantity_milli) * factor),
      stock = await r.availableStock(i.warehouseId,line.product_id),
      incoming = (await r.incomingSupply(a.companyId,i.warehouseId,i.dueDate,line.product_id)).results,
      incomingMilli = incoming.reduce((sum, source) => sum + Number(source.incoming_milli), 0), available = Number(stock?.available ?? 0);
    context.stock!.push({ productId: String(line.product_id), version: Number(stock?.version ?? 0) });
    context.incomingSupply!.push(...incoming.map(source => ({ id: String(source.id), version: Number(source.version), productId: String(line.product_id), quantityMilli: Number(source.incoming_milli), dueDate: String(source.due_date) })));
    shortages.push({ productId: String(line.product_id), requiredMilli: required, availableMilli: available, incomingMilli, shortageMilli: Math.max(0, required - available - incomingMilli) });
  }
  const capacity = await ports.calculateCapacity( a, { specificationId: i.specificationId, branchId: String(warehouse.branch_id), factor,
    startDate: context.startDate!, dueDate: i.dueDate }),
    requiredMinutes = capacity?.requiredMinutes ?? Math.ceil(demand / 1000) * i.manualMinutesPerUnit,
    days = Math.max(0, Math.ceil((requiredMinutes - i.capacityMinutes) / 480)),
    feasible = capacity?.feasibleDate ?? new Date(Date.parse(i.dueDate + "T00:00:00Z") + days * 86400000).toISOString().slice(0, 10),
    capacityMinutes = capacity?.capacityMinutes ?? i.capacityMinutes;
  context.capacityBasis = capacity ? "approved_routing_resource_calendar" : "manual_capacity_assumption";
  context.allocations = capacity?.allocations ?? [];
  context.operations = capacity?.operations ?? [];
  const now = ports.clock.now(), result = { id, recommendationId, status: "calculated", version: 1, shortages, feasibleDate: feasible,
    requiredMinutes, capacityMinutes, context };
  const guards = ports.contextGuards( a, key, context);
  if (i.comparisonScenarioId) {
    const comparison = await r.comparison(i.comparisonScenarioId,a.companyId,warehouse.branch_id,i.specificationId);
    if (!comparison) throw new BusinessRuleError(404, "PLANNING_COMPARISON_NOT_FOUND", "Compare a scenario for the same output, branch and specification.");
    const previousShortages = JSON.parse(String(comparison.shortages_json)) as Array<{ shortageMilli: number }>;
    context.comparison = { scenarioId: String(comparison.id), sourceVersion: Number(comparison.version),
      shortageDeltaMilli: shortages.reduce((n, shortage) => n + shortage.shortageMilli, 0) - previousShortages.reduce((n, shortage) => n + shortage.shortageMilli, 0),
      requiredMinutesDelta: requiredMinutes - Number(comparison.required_minutes), feasibleDate: String(comparison.feasible_date), currentFeasibleDate: feasible };
    guards.push(r.guardComparison(key,"comparison",i.comparisonScenarioId,a.companyId,String(warehouse.branch_id),i.specificationId,Number(comparison.version)));
  }
  return ports.commands.commit( a, key, op, i, id, 1, result, [
    r.guardScenarioSpecification(key,"specification-source",i.specificationId,a.companyId,i.specificationVersion),
    ...guards,
    r.prepareScenario(id,a.companyId,warehouse.branch_id,i.warehouseId,i.specificationId,i.specificationVersion,demand,i.dueDate,capacityMinutes,JSON.stringify(shortages),a.userId,now,now,JSON.stringify(context)),
    r.prepareRecommendation(recommendationId,id,1,demand,requiredMinutes,feasible,a.userId,now,now),
    r.prepareCalculateEvent(`planning-event-${key}`,id,a.userId,context.demandSource ? "Calculated from confirmed Sales demand and captured supply/capacity" : "Calculated from declared manual demand and captured supply/capacity",now),
  ]);
}
async function cancelPlanningScenario(a: WorkContext, id: string, raw: unknown, key: string) {
  access(a, "planning", true);
  const i = z.object({ action: z.literal("cancel"), expectedVersion: version, reason }).parse(raw),
    op = "planning.scenario.cancel", prior = await ports.commands.replay( a, key, op, id, i);
  if (prior) return prior;
  const row = await r.scenario(id,a.companyId,a.branchId,a.branchId);
  if (!row) throw new BusinessRuleError(404, "PLANNING_SCENARIO_NOT_FOUND", "Scenario not found in this planning scope.");
  if (row.created_by !== a.userId && !hasPermission(a, "records.review")) throw new BusinessRuleError(403, "PLANNING_OWNER_REQUIRED", "The planner or an authorized reviewer must cancel this scenario.");
  if (row.status !== "calculated" || Number(row.version) !== i.expectedVersion) throw new BusinessRuleError(409, "PLANNING_SCENARIO_RELEASED", "Only the current unreleased scenario can be cancelled. Cancel a released order through Manufacturing.");
  const now = ports.clock.now();
  return ports.commands.commit( a, key, op, i, id, i.expectedVersion + 1, { id, status: "cancelled", version: i.expectedVersion + 1 }, [
    r.lockScenarioRecommendations(id),
    r.guardUnreleasedScenario(key,"scenario",id,a.companyId,i.expectedVersion),
    r.prepareScenarioCancellation(now,id),
    r.prepareRecommendationCancellation(a.userId,now,id),
    r.prepareCancelEvent(`planning-event-${key}`,id,i.expectedVersion + 1,a.userId,i.reason,now),
  ]);
}
async function reviewPlanningRecommendation(
  a: WorkContext,
  id: string,
  raw: unknown,
  key: string,
) {
  access(a, "planning", true);
  ports.requirePermission(a, "records.review");
  const i = recommendationSchema.parse(raw), op = `planning.recommendation.${i.action}`,
    prior = await ports.commands.replay( a, key, op, id, i);
  if (prior) return prior;
  const row = await r.scopedRecommendation(id,a.companyId,a.branchId,a.branchId);
  if (!row)
    throw new BusinessRuleError(
      404,
      "PLANNING_RECOMMENDATION_NOT_FOUND",
      "Recommendation not found.",
    );
  if (
    row.status !== "proposed" ||
    Number(row.version) !== i.expectedVersion ||
    row.created_by === a.userId
  )
    throw new BusinessRuleError(
      409,
      "CONFLICT",
      "Independent current review is required.",
    );
  const status = i.action === "approve" ? "approved" : "rejected";
  const now = ports.clock.now(),
    steps: Effect[] = [
      r.lockRecommendation(id),
      r.guardIndependentRecommendation(key,"recommendation",id,i.expectedVersion,a.userId),
      r.prepareRecommendationReview(status,a.userId,now,id),
    ];
  let orderId: null | string = null;
  if (i.action === "approve") {
    const context = JSON.parse(String(row.context_json)) as PlanningContext;
    steps.push(...ports.contextGuards( a, key, context),
      r.guardRecommendationSpecification(key,"specification-source",String(row.specification_id),a.companyId,Number(row.specification_version)));
    for (const [n, allocation] of (context.allocations ?? []).entries()) steps.push(
      r.guardCapacityReservation(key,`capacity-reservation-${n}`,allocation.resourceId,allocation.workDate,allocation.minutes,allocation.availableMinutes),
      r.prepareCapacityReservation(`planning-capacity-${key}-${n}`,row.scenario_id,allocation.resourceId,allocation.workDate,allocation.minutes,a.userId,now),
    );
    orderId = `mfg-order-${key}`;
    steps.push(
      r.prepareManufacturingOrder(orderId,a.companyId,row.branch_id,row.warehouse_id,id,row.specification_id,row.specification_version,row.recommended_quantity_milli,a.userId,now,now),
      r.prepareScenarioRelease(now,row.scenario_id),
      r.prepareReleaseEvent(`planning-event-${key}`,a.userId,i.reason,now,row.scenario_id),
      r.prepareManufacturingReleaseEvent(`mfg-event-${key}`,orderId,a.userId,i.reason,now),
    );
  }
  return ports.commands.commit(
    a,
    key,
    op,
    i,
    id,
    i.expectedVersion + 1,
    { id, status, version: i.expectedVersion + 1, orderId },
    steps,
  );
}return {productionWorkspace,planningExecutiveEvidenceRow,createPlanningScenario,cancelPlanningScenario,reviewPlanningRecommendation};}
