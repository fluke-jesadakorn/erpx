import {productionReadProjection as productioncommerceProjection} from '@erpx/server-commerce/public/production-read-projections';
import {productionReadProjection as productionfinanceProjection} from '@erpx/server-finance/public/production-read-projections';
import {productionReadProjection as productionplanningProjection} from '@erpx/server-planning/public/production-read-projections';
import {productionReadProjection as productioninventoryProjection} from '@erpx/server-inventory/public/production-read-projections';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';import type {Row} from '@erpx/server-commerce/public/commerce';import {assertion} from "@erpx/server-workflow/public/command-assertions";import {ownerWrite} from '@erpx/server-manufacturing/adapters/native-owner-write';

/** Owner-local fixed read: the latest output-report actor on a production
 * order. `lastReportActor` (QC pass) and `lastOutputReportActor` (scrap approval)
 * execute the identical statement on the caller's open production unit; one
 * definition keeps those two atomic QC witnesses from drifting and removes the
 * redundant TypeScript SQL. This read is deliberately outside the RD_*
 * read-plan registry (it is an atomic witness, not a projection) and stays
 * live TypeScript on the same transaction and snapshot as before. */
const reportActorSelect=(suffix:string)=>`SELECT actor_id FROM manufacturing_order_events WHERE order_id=? AND action='report'${suffix}`;
const latestReportActorSelect=reportActorSelect(" ORDER BY version DESC LIMIT 1");

/** Owner-local fixed guard SQL: the approved-specification witness used by
 * scenario and recommendation purchasing. Both guards execute the identical
 * EXISTS on the caller's open production unit; one definition keeps them from
 * drifting and removes the redundant TypeScript SQL. It stays a live statement
 * inside its assertion on the same transaction and snapshot (no fail-closed
 * live-read replacement) and is deliberately outside the RD_* read-plan
 * registry (an atomic witness, not a projection). */
const approvedSpecGuardSql =
  "EXISTS(SELECT 1 FROM manufacturing_specifications WHERE id=? AND company_id=? AND version=? AND status='approved')";

/** Owner-local fixed guard SQL: the independent-QC witness shared by the pass
 * and fail transition guards. Both guards execute the identical EXISTS on the
 * caller's open production unit; one definition keeps them from drifting and
 * removes the redundant TypeScript SQL. It stays a live statement inside its
 * assertion on the same transaction and snapshot (no fail-closed live-read
 * replacement) and is deliberately outside the RD_* read-plan registry (an
 * atomic witness, not a projection). */
const independentQcGuardSql =
  "EXISTS(SELECT 1 FROM manufacturing_order_events WHERE order_id=? AND action='report' AND version=? AND actor_id<>?)";

/** Owner-local fixed guard SQL: the completed-operation witness shared by the
 * routing-completion and operation-sequence guards. Both guards execute the
 * identical nested EXISTS on the caller's open production unit; one definition
 * keeps them from drifting and removes the redundant TypeScript SQL. It stays a
 * live statement inside its assertion on the same transaction and snapshot (no
 * fail-closed live-read replacement) and is deliberately outside the RD_* read
 * plan registry (an atomic witness, not a projection). */
const operationEventDoneGuardSql =
  "AND NOT EXISTS(SELECT 1 FROM manufacturing_operation_events e WHERE e.order_id=? AND e.operation_id=op.id))";

export function productionCommandSources(db:SqlDatabase){return {async specification(value:(string | number)[],v:number|undefined){return db
    .prepare(
      `SELECT * FROM manufacturing_specifications WHERE id=? AND company_id=? ${v ? "AND version=?" : ""}`,
    )
    .bind(...value)
    .first<Row>();},
async specifications(companyId:string){return db
          .prepare(
            "SELECT * FROM manufacturing_specifications WHERE company_id=? ORDER BY updated_at DESC",
          )
          .bind(companyId)
          .all<Row>();},
async orders(companyId:string,branchId:string | null,branchId2:string | null){return db
          .prepare(
            `SELECT o.*,s.name specification_name,p.name output_name FROM manufacturing_orders o JOIN manufacturing_specifications s ON s.id=o.specification_id ${productioncommerceProjection.outputProduct} WHERE o.company_id=? AND (?::text IS NULL OR o.branch_id=?) ORDER BY o.updated_at DESC`,
          )
          .bind(companyId, branchId, branchId2)
          .all<Row>();},
async accountingProfiles(companyId:string){return db.prepare("SELECT id,status,version,currency,chart_id,inventory_account_id,wip_account_id,labor_account_id,overhead_account_id FROM manufacturing_accounting_profiles WHERE company_id=? ORDER BY updated_at DESC")
          .bind(companyId).all<Row>();},
async specificationLines(specificationId:string){return db.prepare("SELECT * FROM manufacturing_specification_lines WHERE specification_id=? ORDER BY ordinal")
      .bind(specificationId).all<Row>();},
guardScenarioSpecification(key:string,value:"specification-source",specificationId:string,companyId:string,specificationVersion:number){return assertion(db, key, value, approvedSpecGuardSql, [specificationId, companyId, specificationVersion]);},
guardRecommendationSpecification(key:string,value:"specification-source",value2:string,companyId:string,value3:number){return assertion(db, key, value, approvedSpecGuardSql, [value2, companyId, value3]);},
 prepareManufacturingOrder(orderId:string,companyId:string,branch_id:string | number | null,warehouse_id:string | number | null,id:string,specification_id:string | number | null,specification_version:string | number | null,recommended_quantity_milli:string | number | null,userId:string,now:number,now2:number){return ownerWrite('order.create',[orderId,companyId,branch_id,warehouse_id,id,specification_id,specification_version,recommended_quantity_milli,userId,now,now2],String(orderId));},
 prepareManufacturingReleaseEvent(value:string,orderId:string,userId:string,reason:string,now:number){return ownerWrite('order.release-event',[value,orderId,userId,reason,now],String(orderId));},
async scopedOrder(id:string,companyId:string,branchId:string | null,branchId2:string | null){return db
      .prepare(
        `SELECT o.*,s.output_product_id,s.batch_quantity_milli,p.tracking_mode output_tracking_mode FROM manufacturing_orders o JOIN manufacturing_specifications s ON s.id=o.specification_id ${productioncommerceProjection.outputProduct} WHERE o.id=? AND o.company_id=? AND (?::text IS NULL OR o.branch_id=?)`,
      )
      .bind(id, companyId, branchId, branchId2)
      .first<Row>();},
async orderSpecificationLines(specification_id:string | number | null){return db
        .prepare(
          `SELECT l.*,p.tracking_mode FROM manufacturing_specification_lines l ${productioncommerceProjection.materialProduct} WHERE l.specification_id=? ORDER BY l.ordinal`,
        )
        .bind(specification_id)
        .all<Row>();},
lockOrder(id:string){return db.prepare("SELECT id FROM manufacturing_orders WHERE id=? FOR UPDATE").bind(id);},
guardOrderVersion(key:string,value:"order",id:string,expectedVersion:number,status:string | number | null){return assertion(db, key, value, "EXISTS(SELECT 1 FROM manufacturing_orders WHERE id=? AND version=? AND status=?)", [id, expectedVersion, status]);},
 prepareMaterialReservation(value:string,id:string,warehouse_id:string | number | null,product_id:string | number | null,required:number,expiresAt:number,userId:string,now:number){return ownerWrite('order.reservation',[value,id,warehouse_id,product_id,required,expiresAt,userId,now],String(id));},
async materialReservation(id:string,product_id:string | number | null){return db
          .prepare(
            "SELECT * FROM manufacturing_material_reservations WHERE order_id=? AND product_id=?",
          )
          .bind(id, product_id)
          .first<Row>();},
guardCurrentMaterialReservation(key:string,value:string,id:string | number | null,now:number){return assertion(
          db,
          key,
          value,
          "EXISTS(SELECT 1 FROM manufacturing_material_reservations WHERE id=? AND consumed_milli=0 AND released_milli=0 AND expires_at>?)",
          [id, now],
        );},
guardCompleteRouting(key:string,value:"completed-routing",value2:string,id:string){return assertion(db, key, value, `NOT EXISTS(SELECT 1 FROM manufacturing_specification_operations op WHERE op.specification_id=? ${operationEventDoneGuardSql}`, [value2, id]);},
 prepareOutputReport(value:number,value2:number,value3:number,value4:number,id:string){return ownerWrite('order.output-report',[value,value2,value3,value4,id],String(id));},
async lastReportActor(id:string){return db
      .prepare(latestReportActorSelect)
      .bind(id)
      .first<string>("actor_id");},
guardIndependentPass(key:string,value:"independent-qc",id:string,expectedVersion:number,userId:string){return assertion(db, key, value, independentQcGuardSql, [id, expectedVersion, userId]);},
async currentReportActor(id:string,expectedVersion:number){return db.prepare(reportActorSelect(" AND version=?"))
      .bind(id, expectedVersion).first<string>("actor_id");},
guardIndependentFail(key:string,value:"independent-qc",id:string,expectedVersion:number,userId:string){return assertion(db, key, value, independentQcGuardSql, [id, expectedVersion, userId]);},
async lastOutputReportActor(id:string){return db.prepare(latestReportActorSelect).bind(id).first<string>("actor_id");},
async operation(operationId:string,specification_id:string | number | null){return db.prepare("SELECT * FROM manufacturing_specification_operations WHERE id=? AND specification_id=?")
      .bind(operationId, specification_id).first<Row>();},
guardOperationSequence(key:string,value:"routing-sequence",value2:string,value3:number,id:string){return assertion(db, key, value, `NOT EXISTS(SELECT 1 FROM manufacturing_specification_operations op WHERE op.specification_id=? AND op.ordinal<? ${operationEventDoneGuardSql}`, [value2, value3, id]);},
 prepareOperationEvent(value:string,id:string,operationId:string,next:number,laborMinutes:number,machineMinutes:number,reason:string,userId:string,now:number){return ownerWrite('order.operation-event',[value,id,operationId,next,laborMinutes,machineMinutes,reason,userId,now],String(id));},
 prepareReservationRelease(id:string){return ownerWrite('order.reservation-release',[id],String(id));},
 prepareOrderTransition(status:string,now:number,id:string){return ownerWrite('order.transition',[status,now,id],String(id));},
 prepareOrderEvent(value:string,id:string,next:number,action:"cancel" | "reserve" | "issue" | "report" | "pass-qc" | "fail-qc" | "rework" | "scrap" | "complete-operation",userId:string,reason:string,status:string,value2:string,now:number){return ownerWrite('order.event',[value,id,next,action,userId,reason,status,value2,now],String(id));},
async detailedOrder(id:string,companyId:string,branchId:string | null,branchId2:string | null){return db
      .prepare(
        `SELECT o.*,s.name specification_name,s.mode,p.name output_name,p.tracking_mode output_tracking_mode FROM manufacturing_orders o JOIN manufacturing_specifications s ON s.id=o.specification_id ${productioncommerceProjection.outputProduct} WHERE o.id=? AND o.company_id=? AND (?::text IS NULL OR o.branch_id=?)`,
      )
      .bind(id, companyId, branchId, branchId2)
      .first<Row>();},
async orderReservations(id:string){return db
        .prepare(
          `SELECT r.*,p.name product_name,p.tracking_mode FROM manufacturing_material_reservations r ${productioncommerceProjection.reservationProduct} WHERE r.order_id=? ORDER BY p.name`,
        )
        .bind(id)
        .all<Row>();},
async orderEvents(id:string){return db
        .prepare(
          "SELECT * FROM manufacturing_order_events WHERE order_id=? ORDER BY version",
        )
        .bind(id)
        .all<Row>();},
async costSnapshot(id:string){return db
      .prepare("SELECT * FROM manufacturing_cost_snapshots WHERE order_id=?")
      .bind(id)
      .first<Row>();},
async productGenealogy(id:string){return db
        .prepare("SELECT * FROM manufacturing_genealogy WHERE order_id=?")
        .bind(id)
        .all<Row>();},
async orderPostings(id:string){return db.prepare(`SELECT p.*,j.status journal_status FROM manufacturing_order_postings p ${productionfinanceProjection.journal} WHERE p.order_id=? ORDER BY p.source_order_version`)
      .bind(id).all<Row>();},
async orderOperations(id:string,specification_id:string | number | null){return db.prepare(`SELECT op.*,r.name resource_name,e.labor_minutes,e.machine_minutes,e.evidence,e.created_at completed_at
      FROM manufacturing_specification_operations op ${productionplanningProjection.operationResource}
      LEFT JOIN manufacturing_operation_events e ON e.operation_id=op.id AND e.order_id=? WHERE op.specification_id=? ORDER BY op.ordinal`)
      .bind(id, specification_id).all<Row>();},
async lotGenealogy(id:string){return db.prepare(`SELECT g.*,i.lot_id input_lot_id,il.lot_code input_lot_code,il.serial_number input_serial_number,
      ol.lot_id output_lot_id,l.lot_code output_lot_code,l.serial_number output_serial_number FROM manufacturing_lot_genealogy g
      JOIN manufacturing_material_lot_issues i ON i.id=g.input_issue_id ${productioninventoryProjection.genealogyInputLot}
      JOIN manufacturing_output_lots ol ON ol.id=g.output_lot_id ${productioninventoryProjection.genealogyOutputLot} WHERE g.order_id=? ORDER BY g.id`)
      .bind(id).all<Row>();},
async openOrders(companyId:string,branchId:string | null,branchId2:string | null){return db
        .prepare(
          "SELECT o.*,s.name specification_name FROM manufacturing_orders o JOIN manufacturing_specifications s ON s.id=o.specification_id WHERE o.company_id=? AND o.status NOT IN ('completed','cancelled') AND (?::text IS NULL OR o.branch_id=?) ORDER BY o.updated_at DESC",
        )
        .bind(companyId, branchId, branchId2)
        .all<Row>();}};}
