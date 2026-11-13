import type {WorkContext} from '@erpx/server-identity_access/public/authorization';import type {Row} from '@erpx/server-commerce/public/commerce';import type {WorkItem} from "@erpx/feature-work_projection/lib/work-item";import {hasModuleAccess} from '@erpx/catalogue/module-access';import {decimalToMinor} from "@erpx/feature-finance/lib/finance-money";import {BusinessRuleError,type Clock} from '@erpx/server-shared/index';import type {prepareAttachmentBinding} from '@erpx/server-files/public/attachments/service';import type {prepareManufacturingPosting} from '@erpx/server-manufacturing/public/production-accounting';import {orderSchema,q} from '@erpx/server-manufacturing/domain/production';export interface ManufacturingProductionRepository<Effect>{scopedOrder(id:string,companyId:string,branchId:string | null,branchId2:string | null):Promise<Row|null>;
orderSpecificationLines(specification_id:string | number | null):Promise<{results:Row[]}>;
lockOrder(id:string):Effect;
guardOrderVersion(key:string,value:"order",id:string,expectedVersion:number,status:string | number | null):Effect;
lockReserveStock(warehouse_id:string | number | null,product_id:string | number | null):Effect;
guardReserveAvailability(key:string,value:string,warehouse_id:string | number | null,product_id:string | number | null,warehouse_id2:string | number | null,product_id2:string | number | null,required:number):Effect;
prepareMaterialReservation(value:string,id:string,warehouse_id:string | number | null,product_id:string | number | null,required:number,expiresAt:number,userId:string,now:number):Effect;
materialReservation(id:string,product_id:string | number | null):Promise<Row|null>;
materialStock(warehouse_id:string | number | null,product_id:string | number | null):Promise<Row|null>;
lockIssueStock(warehouse_id:string | number | null,product_id:string | number | null):Effect;
guardIssueStock(key:string,value:string,warehouse_id:string | number | null,product_id:string | number | null,value2:number,amount:number,value3:number):Effect;
guardCurrentMaterialReservation(key:string,value:string,id:string | number | null,now:number):Effect;
prepareReservationConsumption(id:string | number | null):Effect;
prepareStockIssue(amount:number,value:number,warehouse_id:string | number | null,product_id:string | number | null):Effect;
guardIssueDate(key:string,value:string,warehouse_id:string | number | null,product_id:string | number | null,entryDate:string):Effect;
prepareIssueMove(value:string,id:string,warehouse_id:string | number | null,product_id:string | number | null,next:number,value2:number,value3:number,entryDate:string,userId:string,now:number):Effect;
prepareMaterialCost(material:number,id:string):Effect;
guardCompleteRouting(key:string,value:"completed-routing",value2:string,id:string):Effect;
prepareOutputReport(value:number,value2:number,value3:number,value4:number,id:string):Effect;
lastReportActor(id:string):Promise<string|null>;
guardIndependentPass(key:string,value:"independent-qc",id:string,expectedVersion:number,userId:string):Effect;
prepareOutputStock(warehouse_id:string | number | null,output_product_id:string | number | null,output:number,total:number):Effect;
guardOutputDate(key:string,value:"output-movement-date",warehouse_id:string | number | null,output_product_id:string | number | null,entryDate:string):Effect;
prepareOutputMove(value:string,id:string,warehouse_id:string | number | null,output_product_id:string | number | null,next:number,output:number,total:number,entryDate:string,userId:string,now:number):Effect;
prepareCostSnapshot(value:string,id:string,next:number,material_cost_minor:string | number | null,labor_cost_minor:string | number | null,overhead_cost_minor:string | number | null,total:number,output:number,now:number):Effect;
prepareProductGenealogy(value:string,id:string,product_id:string | number | null,output_product_id:string | number | null,value2:number,output:number,next:number,now:number):Effect;
currentReportActor(id:string,expectedVersion:number):Promise<string|null>;
guardIndependentFail(key:string,value:"independent-qc",id:string,expectedVersion:number,userId:string):Effect;
lastOutputReportActor(id:string):Promise<string|null>;
operation(operationId:string,specification_id:string | number | null):Promise<Row|null>;
guardOperationSequence(key:string,value:"routing-sequence",value2:string,value3:number,id:string):Effect;
prepareOperationEvent(value:string,id:string,operationId:string,next:number,laborMinutes:number,machineMinutes:number,reason:string,userId:string,now:number):Effect;
prepareReservationRelease(id:string):Effect;
prepareCapacityRelease(now:number,id:string):Effect;
attachmentSets(companyId:string,attachmentSetIds:string[]):Promise<{results:{id:string;purpose_key:string;component_kind:string;component_ref:string|null}[]}>;
prepareOrderTransition(status:string,now:number,id:string):Effect;
prepareOrderEvent(value:string,id:string,next:number,action:"cancel" | "reserve" | "issue" | "report" | "pass-qc" | "fail-qc" | "rework" | "scrap" | "complete-operation",userId:string,reason:string,status:string,value2:string,now:number):Effect;
detailedOrder(id:string,companyId:string,branchId:string | null,branchId2:string | null):Promise<Row|null>;
orderReservations(id:string):Promise<{results:Row[]}>;
orderEvents(id:string):Promise<{results:Row[]}>;
costSnapshot(id:string):Promise<Row|null>;
productGenealogy(id:string):Promise<{results:Row[]}>;
orderPostings(id:string):Promise<{results:Row[]}>;
orderOperations(id:string,specification_id:string | number | null):Promise<{results:Row[]}>;
availableMaterialLots(specification_id:string | number | null,companyId:string,warehouse_id:string | number | null):Promise<{results:Row[]}>;
lotGenealogy(id:string):Promise<{results:Row[]}>;
openOrders(companyId:string,branchId:string | null,branchId2:string | null):Promise<{results:Row[]}>;}export interface OwnerCommands<Effect>{replay<Result=Record<string,unknown>>(actor:WorkContext,key:string,operation:string,id:string,input:unknown):Promise<Result|null>;commit<Result>(actor:WorkContext,key:string,operation:string,input:unknown,id:string,version:number,result:Result,effects:Effect[],scope?:Pick<WorkContext,'companyId'|'branchId'|'departmentId'>):Promise<Result>;}export interface ManufacturingProductionPorts<Effect>{repository:ManufacturingProductionRepository<Effect>;commands:OwnerCommands<Effect>;clock:Clock;requirePermission(actor:WorkContext,permission:string):WorkContext;prepareAttachment(actor:WorkContext,input:Parameters<typeof prepareAttachmentBinding>[2]):Promise<{guards:Effect[];steps:Effect[]}>;preparePosting(actor:WorkContext,input:Parameters<typeof prepareManufacturingPosting>[2]):Promise<{steps:Effect[];journalId:string|null}>;}export function createManufacturingProductionApplication<Effect>(ports:ManufacturingProductionPorts<Effect>){const r=ports.repository;function access(
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
async function changeManufacturingOrder(
  a: WorkContext,
  id: string,
  raw: unknown,
  key: string,
) {
  access(a, "manufacturing", true);
  const i = orderSchema.parse(raw),
    row = await r.scopedOrder(id,a.companyId,a.branchId,a.branchId);
  if (!row)
    throw new BusinessRuleError(
      404,
      "MANUFACTURING_ORDER_NOT_FOUND",
      "Order not found.",
    );
  const op = `manufacturing.order.${i.action}`,
    prior = await ports.commands.replay( a, key, op, id, i);
  if (prior) return prior;
  if (Number(row.version) !== i.expectedVersion)
    throw new BusinessRuleError(409, "CONFLICT", "Order changed. Reload and retry.");
  const now = ports.clock.now(),
    next = i.expectedVersion + 1,
    lines = (
      await r.orderSpecificationLines(row.specification_id)
    ).results,
    factor =
      Number(row.planned_quantity_milli) / Number(row.batch_quantity_milli),
    steps: Effect[] = [
      r.lockOrder(id),
      r.guardOrderVersion(key,"order",id,i.expectedVersion,row.status),
    ], entryDate = i.entryDate ?? new Date(now).toISOString().slice(0, 10);
  let status = "";
  let journalId: string | null = null;
  if (i.action === "reserve") {
    if (row.status !== "planned" || i.expiresAt <= now)
      throw new BusinessRuleError(
        409,
        "INVALID_MANUFACTURING_TRANSITION",
        "Reserve a planned order with a future expiry.",
      );
    status = "reserved";
    for (const [n, line] of lines.entries()) {
      const required = Math.ceil(Number(line.quantity_milli) * factor);
      steps.push(
        r.lockReserveStock(row.warehouse_id,line.product_id),
        r.guardReserveAvailability(key,`available-${n}`,row.warehouse_id,line.product_id,row.warehouse_id,line.product_id,required),
        r.prepareMaterialReservation(`mfg-reservation-${key}-${n}`,id,row.warehouse_id,line.product_id,required,i.expiresAt,a.userId,now),
      );
    }
  } else if (i.action === "issue" || i.action === "pass-qc") {
    // Material issue and independent QC acceptance own their lot posting in
    // the native Manufacturing order command. The route dispatches both
    // actions there; reaching this retained application is a configuration
    // failure rather than a silent lot-less write.
    throw new BusinessRuleError(
      503,
      "NATIVE_MANUFACTURING_REQUIRED",
      "The native production command is unavailable.",
    );
  } else if (i.action === "report") {
    if (row.status !== "in_progress")
      throw new BusinessRuleError(
        409,
        "INVALID_MANUFACTURING_TRANSITION",
        "Report an in-progress order.",
      );
    status = "awaiting_qc";
    if (q(i.outputQuantity) <= 0) throw new BusinessRuleError(422, "MANUFACTURING_OUTPUT_REQUIRED", "Report a positive output quantity; independently scrap failed output when no usable output remains.");
    if (row.output_tracking_mode === "serial" && q(i.outputQuantity) % 1000 !== 0) throw new BusinessRuleError(422, "SERIAL_QUANTITY_INVALID", "Serialized finished output must contain whole units.");
    steps.push(r.guardCompleteRouting(key,"completed-routing",String(row.specification_id),id));
    const labor = decimalToMinor(i.laborCost), overhead = decimalToMinor(i.overheadCost);
    steps.push(
      r.prepareOutputReport(q(i.outputQuantity),q(i.scrapQuantity),Number(row.labor_cost_minor) + labor,Number(row.overhead_cost_minor) + overhead,id),
    );
    const posting = await ports.preparePosting( a, { key, orderId: id, orderVersion: next, phase: "report", entryDate, labor, overhead });
    steps.push(...posting.steps); journalId = posting.journalId;
  } else if (i.action === "fail-qc") {
    ports.requirePermission(a, "records.review");
    if (row.status !== "awaiting_qc")
      throw new BusinessRuleError(
        409,
        "INVALID_MANUFACTURING_TRANSITION",
        "QC reviews reported output.",
      );
    status = "qc_failed";
    const reporter = await r.currentReportActor(id,i.expectedVersion);
    if (!reporter || reporter === a.userId) throw new BusinessRuleError(409, "CONFLICT", "QC must be independent from output reporting.");
    steps.push(r.guardIndependentFail(key,"independent-qc",id,i.expectedVersion,a.userId));
  } else if (i.action === "scrap") {
    ports.requirePermission(a, "records.review");
    if (row.status !== "qc_failed") throw new BusinessRuleError(409, "INVALID_MANUFACTURING_TRANSITION", "Only independently failed output can be written off.");
    const reporter = await r.lastOutputReportActor(id);
    if (!reporter || reporter === a.userId) throw new BusinessRuleError(409, "CONFLICT", "The reporter cannot approve failed output disposal.");
    status = "cancelled";
    const total = Number(row.material_cost_minor) + Number(row.labor_cost_minor) + Number(row.overhead_cost_minor),
      posting = await ports.preparePosting( a, { key, orderId: id, orderVersion: next, phase: "scrap", entryDate, output: total });
    steps.push(...posting.steps); journalId = posting.journalId;
  } else if (i.action === "complete-operation") {
    if (row.status !== "in_progress") throw new BusinessRuleError(409, "INVALID_MANUFACTURING_TRANSITION", "Routing operations belong to an in-progress run.");
    const operation = await r.operation(i.operationId,row.specification_id);
    if (!operation) throw new BusinessRuleError(404, "MANUFACTURING_OPERATION_NOT_FOUND", "Routing operation not found in this run.");
    steps.push(r.guardOperationSequence(key,"routing-sequence",String(row.specification_id),Number(operation.ordinal),id),
      r.prepareOperationEvent(`mfg-operation-event-${key}`,id,i.operationId,next,i.laborMinutes,i.machineMinutes,i.reason,a.userId,now));
    status = "in_progress";
  } else if (i.action === "rework") {
    if (row.status !== "qc_failed")
      throw new BusinessRuleError(
        409,
        "INVALID_MANUFACTURING_TRANSITION",
        "Only failed output can re-enter work.",
      );
    status = "in_progress";
  } else {
    if (!["planned", "reserved"].includes(String(row.status)))
      throw new BusinessRuleError(
        409,
        "INVALID_MANUFACTURING_TRANSITION",
        "Only unissued orders can be cancelled.",
      );
    status = "cancelled";
    steps.push(
      r.prepareReservationRelease(id),
    );
  }
  if (["completed", "cancelled"].includes(status)) steps.push(r.prepareCapacityRelease(now,id));
  const attachmentPurpose = i.action === "report"
    ? "run_report_evidence"
    : ["pass-qc", "fail-qc"].includes(i.action)
      ? "qc_evidence"
      : i.action === "rework"
        ? "rework_action_evidence"
        : null;
  if (i.attachmentSetIds.length && !attachmentPurpose)
    throw new BusinessRuleError(
      422,
      "MANUFACTURING_ATTACHMENT_ACTION",
      "Files can be bound only to output reporting, QC review, or rework.",
    );
  let attachmentBinding: null | {guards:Effect[];steps:Effect[]} = null;
  if (attachmentPurpose && i.attachmentSetIds.length) {
    const componentRef = i.action === "report" ? "output-report" : i.action === "rework" ? "rework" : "qc-review";
    const sets = (await r.attachmentSets(a.companyId,i.attachmentSetIds)).results;
    if (
      sets.length !== new Set(i.attachmentSetIds).size ||
      sets.some((set) => set.purpose_key !== attachmentPurpose || set.component_kind !== "action" || set.component_ref !== componentRef)
    )
      throw new BusinessRuleError(
        422,
        "MANUFACTURING_ATTACHMENT_PURPOSE",
        "Choose only evidence prepared for this exact production action.",
      );
    attachmentBinding = await ports.prepareAttachment( a, {
        attachmentSetIds: i.attachmentSetIds,
        bindingContract: {
          targetType: "production_run",
          bindingType: `manufacturing_${i.action}`,
          contexts: [{ purposeKey: attachmentPurpose, componentKind: "action", componentRef }],
        },
        targetId: id,
        expectedTargetVersion: i.expectedVersion,
        commandId: key,
        bindingId: `mfg-event-${key}`,
      });
  }
  if (attachmentBinding) steps.unshift(...attachmentBinding.guards);
  steps.push(
    r.prepareOrderTransition(status,now,id),
    r.prepareOrderEvent(`mfg-event-${key}`,id,next,i.action,a.userId,i.reason,status,JSON.stringify(i),now),
  );
  if (attachmentBinding) steps.push(...attachmentBinding.steps);
  return ports.commands.commit(
    a,
    key,
    op,
    i,
    id,
    next,
    { id, status, version: next, journalId },
    steps,
  );
}
async function productionDetail(a: WorkContext, id: string) {
  access(a, "manufacturing");
  const 
    order = await r.detailedOrder(id,a.companyId,a.branchId,a.branchId);
  if (!order)
    throw new BusinessRuleError(
      404,
      "MANUFACTURING_ORDER_NOT_FOUND",
      "Order not found.",
    );
  return {
    order,
    materials: (
      await r.orderReservations(id)
    ).results,
    history: (
      await r.orderEvents(id)
    ).results,
    cost: await r.costSnapshot(id),
    genealogy: (
      await r.productGenealogy(id)
    ).results,
    postings: (await r.orderPostings(id)).results,
    operations: (await r.orderOperations(id,order.specification_id)).results,
    materialLots: (await r.availableMaterialLots(order.specification_id,a.companyId,order.warehouse_id)).results,
    lotGenealogy: (await r.lotGenealogy(id)).results,
  };
}
async function manufacturingWork(a: WorkContext): Promise<WorkItem[]> {
  if (!hasModuleAccess(a.modules, "manufacturing")) return [];
  const 
    rows = (
      await r.openOrders(a.companyId,a.branchId,a.branchId)
    ).results;
  return rows.map((x) => ({
    id: String(x.id),
    type: "manufacturing_order",
    title: `Production · ${x.specification_name}`,
    detail: `${Number(x.planned_quantity_milli) / 1000} planned`,
    href: `/manufacturing/${x.id}`,
    status: String(x.status),
    updatedAt: Number(x.updated_at),
    queue: "todo",
    canAct: true,
    nextAction:
      String(x.status) === "planned"
        ? "Reserve materials"
        : String(x.status) === "reserved"
          ? "Issue materials"
          : String(x.status) === "awaiting_qc"
            ? "Record QC disposition"
            : "Report output",
    relationships: ["team"],
  }));
}return {changeManufacturingOrder,productionDetail,manufacturingWork};}
