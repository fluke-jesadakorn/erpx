import {productionReadProjection as productioncommerceProjection} from '../../commerce/public/production-read-projections';
import {productionReadProjection as productionfinanceProjection} from '../../finance/public/production-read-projections';
import {productionReadProjection as productionplanningProjection} from '../../planning/public/production-read-projections';
import {productionReadProjection as productioninventoryProjection} from '../../inventory/public/production-read-projections';
import type {SqlDatabase} from '../../runtime/public/runtime/types';import type {Row} from '../../commerce/public/commerce';import {assertion} from '../../workflow/public/commands';export function productionCommandSources(db:SqlDatabase){return {async specification(value:(string | number)[],v:number|undefined){return db
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
prepareSpecification(id:string,companyId:string,name:string,mode:"bom" | "formula",outputProductId:string,value:number,expectedYieldBps:number,userId:string,now:number,now2:number){return db
        .prepare(
          "INSERT INTO manufacturing_specifications VALUES(?,?,?,?,?,?,?,'draft',?,NULL,1,?,?)",
        )
        .bind(
          id,
          companyId,
          name,
          mode,
          outputProductId,
          value,
          expectedYieldBps,
          userId,
          now,
          now2,
        );},
prepareSpecificationLine(value:string,id:string,productId:string,value2:number,value3:string | null,n:number){return db
        .prepare(
          "INSERT INTO manufacturing_specification_lines VALUES(?,?,?,?,?,?)",
        )
        .bind(
          value,
          id,
          productId,
          value2,
          value3,
          n,
        );},
prepareSpecificationOperation(value:string,id:string,resourceId:string,name:string,setupMinutes:number,runMinutesPerBatch:number,n:number){return db.prepare("INSERT INTO manufacturing_specification_operations(id,specification_id,resource_id,name,setup_minutes,run_minutes_per_batch,ordinal) VALUES(?,?,?,?,?,?,?)")
      .bind(value, id, resourceId, name, setupMinutes, runMinutesPerBatch, n);},
guardSpecificationReview(key:string,value:"spec",id:string,companyId:string,expectedVersion:number,expected:"approved" | "draft",action:"approve" | "reject" | "retire",userId:string){return assertion(
        db,
        key,
        value,
        "EXISTS(SELECT 1 FROM manufacturing_specifications WHERE id=? AND company_id=? AND version=? AND status=? AND (?='retire' OR created_by<>?))",
        [id, companyId, expectedVersion, expected, action, userId],
      );},
prepareSpecificationReview(status:"approved" | "rejected" | "retired",userId:string,now:number,id:string){return db
        .prepare(
          "UPDATE manufacturing_specifications SET status=?,reviewed_by=?,version=version+1,updated_at=? WHERE id=?",
        )
        .bind(status, userId, now, id);},
async specificationLines(specificationId:string){return db.prepare("SELECT * FROM manufacturing_specification_lines WHERE specification_id=? ORDER BY ordinal")
      .bind(specificationId).all<Row>();},
guardScenarioSpecification(key:string,value:"specification-source",specificationId:string,companyId:string,specificationVersion:number){return assertion(db, key, value, "EXISTS(SELECT 1 FROM manufacturing_specifications WHERE id=? AND company_id=? AND version=? AND status='approved')", [specificationId, companyId, specificationVersion]);},
guardRecommendationSpecification(key:string,value:"specification-source",value2:string,companyId:string,value3:number){return assertion(db, key, value, "EXISTS(SELECT 1 FROM manufacturing_specifications WHERE id=? AND company_id=? AND version=? AND status='approved')", [value2, companyId, value3]);},
prepareManufacturingOrder(orderId:string,companyId:string,branch_id:string | number | null,warehouse_id:string | number | null,id:string,specification_id:string | number | null,specification_version:string | number | null,recommended_quantity_milli:string | number | null,userId:string,now:number,now2:number){return db
        .prepare(
          "INSERT INTO manufacturing_orders(id,company_id,branch_id,warehouse_id,recommendation_id,specification_id,specification_version,planned_quantity_milli,status,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,'planned',?,?,?)",
        )
        .bind(
          orderId,
          companyId,
          branch_id,
          warehouse_id,
          id,
          specification_id,
          specification_version,
          recommended_quantity_milli,
          userId,
          now,
          now2,
        );},
prepareManufacturingReleaseEvent(value:string,orderId:string,userId:string,reason:string,now:number){return db
        .prepare(
          "INSERT INTO manufacturing_order_events VALUES(?,?,1,'release',? ,?,'planned','{}',?)",
        )
        .bind(value, orderId, userId, reason, now);},
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
prepareMaterialReservation(value:string,id:string,warehouse_id:string | number | null,product_id:string | number | null,required:number,expiresAt:number,userId:string,now:number){return db
          .prepare(
            "INSERT INTO manufacturing_material_reservations VALUES(?,?,?,?,?,0,0,?,?,?)",
          )
          .bind(
            value,
            id,
            warehouse_id,
            product_id,
            required,
            expiresAt,
            userId,
            now,
          );},
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
prepareReservationConsumption(id:string | number | null){return db
          .prepare(
            "UPDATE manufacturing_material_reservations SET consumed_milli=quantity_milli WHERE id=?",
          )
          .bind(id);},
prepareIssueMove(value:string,id:string,warehouse_id:string | number | null,product_id:string | number | null,next:number,value2:number,value3:number,entryDate:string,userId:string,now:number){return db.prepare("INSERT INTO manufacturing_inventory_moves(id,order_id,warehouse_id,product_id,phase,source_order_version,quantity_milli,value_minor,movement_date,created_by,created_at) VALUES(?,?,?,?,'issue',?,?,?,?,?,?)")
          .bind(value, id, warehouse_id, product_id, next, value2, value3, entryDate, userId, now);},
prepareMaterialCost(material:number,id:string){return db
        .prepare(
          "UPDATE manufacturing_orders SET material_cost_minor=? WHERE id=?",
        )
        .bind(material, id);},
guardCompleteRouting(key:string,value:"completed-routing",value2:string,id:string){return assertion(db, key, value, `NOT EXISTS(SELECT 1 FROM manufacturing_specification_operations op WHERE op.specification_id=?
      AND NOT EXISTS(SELECT 1 FROM manufacturing_operation_events e WHERE e.order_id=? AND e.operation_id=op.id))`, [value2, id]);},
prepareOutputReport(value:number,value2:number,value3:number,value4:number,id:string){return db
        .prepare(
          "UPDATE manufacturing_orders SET reported_output_milli=?,reported_scrap_milli=?,labor_cost_minor=?,overhead_cost_minor=? WHERE id=?",
        )
        .bind(
          value,
          value2,
          value3,
          value4,
          id,
        );},
async lastReportActor(id:string){return db
      .prepare(
        "SELECT actor_id FROM manufacturing_order_events WHERE order_id=? AND action='report' ORDER BY version DESC LIMIT 1",
      )
      .bind(id)
      .first<string>("actor_id");},
guardIndependentPass(key:string,value:"independent-qc",id:string,expectedVersion:number,userId:string){return assertion(db, key, value, "EXISTS(SELECT 1 FROM manufacturing_order_events WHERE order_id=? AND action='report' AND version=? AND actor_id<>?)", [id, expectedVersion, userId]);},
prepareOutputMove(value:string,id:string,warehouse_id:string | number | null,output_product_id:string | number | null,next:number,output:number,total:number,entryDate:string,userId:string,now:number){return db.prepare("INSERT INTO manufacturing_inventory_moves(id,order_id,warehouse_id,product_id,phase,source_order_version,quantity_milli,value_minor,movement_date,created_by,created_at) VALUES(?,?,?,?,'output',?,?,?,?,?,?)")
        .bind(value, id, warehouse_id, output_product_id, next, output, total, entryDate, userId, now);},
prepareCostSnapshot(value:string,id:string,next:number,material_cost_minor:string | number | null,labor_cost_minor:string | number | null,overhead_cost_minor:string | number | null,total:number,output:number,now:number){return db
        .prepare(
          "INSERT INTO manufacturing_cost_snapshots VALUES(?,?,?,?,?,?,?,?,?)",
        )
        .bind(
          value,
          id,
          next,
          material_cost_minor,
          labor_cost_minor,
          overhead_cost_minor,
          total,
          output,
          now,
        );},
prepareProductGenealogy(value:string,id:string,product_id:string | number | null,output_product_id:string | number | null,value2:number,output:number,next:number,now:number){return db
          .prepare(
            "INSERT INTO manufacturing_genealogy VALUES(?,?,?,?,?,?,?,?)",
          )
          .bind(
            value,
            id,
            product_id,
            output_product_id,
            value2,
            output,
            next,
            now,
          );},
async currentReportActor(id:string,expectedVersion:number){return db.prepare("SELECT actor_id FROM manufacturing_order_events WHERE order_id=? AND action='report' AND version=?")
      .bind(id, expectedVersion).first<string>("actor_id");},
guardIndependentFail(key:string,value:"independent-qc",id:string,expectedVersion:number,userId:string){return assertion(db, key, value, "EXISTS(SELECT 1 FROM manufacturing_order_events WHERE order_id=? AND action='report' AND version=? AND actor_id<>?)", [id, expectedVersion, userId]);},
async lastOutputReportActor(id:string){return db.prepare("SELECT actor_id FROM manufacturing_order_events WHERE order_id=? AND action='report' ORDER BY version DESC LIMIT 1").bind(id).first<string>("actor_id");},
async operation(operationId:string,specification_id:string | number | null){return db.prepare("SELECT * FROM manufacturing_specification_operations WHERE id=? AND specification_id=?")
      .bind(operationId, specification_id).first<Row>();},
guardOperationSequence(key:string,value:"routing-sequence",value2:string,value3:number,id:string){return assertion(db, key, value, `NOT EXISTS(SELECT 1 FROM manufacturing_specification_operations op WHERE op.specification_id=? AND op.ordinal<?
      AND NOT EXISTS(SELECT 1 FROM manufacturing_operation_events e WHERE e.order_id=? AND e.operation_id=op.id))`, [value2, value3, id]);},
prepareOperationEvent(value:string,id:string,operationId:string,next:number,laborMinutes:number,machineMinutes:number,reason:string,userId:string,now:number){return db.prepare("INSERT INTO manufacturing_operation_events(id,order_id,operation_id,action,source_order_version,labor_minutes,machine_minutes,evidence,created_by,created_at) VALUES(?,?,?,'complete',?,?,?,?,?,?)")
        .bind(value, id, operationId, next, laborMinutes, machineMinutes, reason, userId, now);},
prepareReservationRelease(id:string){return db
        .prepare(
          "UPDATE manufacturing_material_reservations SET released_milli=quantity_milli-consumed_milli WHERE order_id=?",
        )
        .bind(id);},
prepareOrderTransition(status:string,now:number,id:string){return db
      .prepare(
        "UPDATE manufacturing_orders SET status=?,version=version+1,updated_at=? WHERE id=?",
      )
      .bind(status, now, id);},
prepareOrderEvent(value:string,id:string,next:number,action:"cancel" | "reserve" | "issue" | "report" | "pass-qc" | "fail-qc" | "rework" | "scrap" | "complete-operation",userId:string,reason:string,status:string,value2:string,now:number){return db
      .prepare(
        "INSERT INTO manufacturing_order_events VALUES(?,?,?,?,?,?,?,?,?)",
      )
      .bind(
        value,
        id,
        next,
        action,
        userId,
        reason,
        status,
        value2,
        now,
      );},
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
