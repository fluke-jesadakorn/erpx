import {inventoryQualityHoldReadSource} from '../../quality/public/inventory-hold-read-source';
import type {SqlDatabase} from '../../runtime/public/runtime/types';import type {Row} from '../../commerce/public/commerce';import {assertion} from '../../workflow/public/commands';export function manufacturingLotSources(db:SqlDatabase){return {async materialLot(lotId:string,companyId:string,productId:string,warehouseId:string){return db.prepare(`SELECT l.*,b.quantity_milli,b.value_minor,b.version balance_version,
      EXISTS(SELECT 1 ${inventoryQualityHoldReadSource()} WHERE h.warehouse_id=b.warehouse_id AND h.lot_id=l.id AND h.status NOT IN ('released','cancelled','disposed')) held
      FROM inventory_lots l JOIN inventory_lot_balances b ON b.lot_id=l.id WHERE l.id=? AND l.company_id=? AND l.product_id=? AND b.warehouse_id=?`)
      .bind(lotId, companyId, productId, warehouseId).first<Row>();},
lockMaterialLot(lotId:string,warehouseId:string){return db.prepare("SELECT lot_id FROM inventory_lot_balances WHERE lot_id=? AND warehouse_id=? FOR UPDATE").bind(lotId, warehouseId);},
guardMaterialLot(key:string,value:string,lotId:string,warehouseId:string,balanceVersion:number,quantity:number,value2:number,entryDate:string){return assertion(db, key, value, `EXISTS(SELECT 1 FROM inventory_lot_balances b JOIN inventory_lots l ON l.id=b.lot_id
        WHERE b.lot_id=? AND b.warehouse_id=? AND b.version=? AND b.quantity_milli>=? AND b.value_minor>=? AND l.status='active'
        AND (l.expiry_date IS NULL OR l.expiry_date>=?) AND NOT EXISTS(SELECT 1 ${inventoryQualityHoldReadSource()} WHERE h.warehouse_id=b.warehouse_id
          AND (h.lot_id=l.id OR (h.lot_id IS NULL AND h.product_id=l.product_id)) AND h.status NOT IN ('released','cancelled','disposed')))`,
        [lotId, warehouseId, balanceVersion, quantity, value2, entryDate]);},
prepareLotConsumption(quantity:number,value:number,lotId:string,warehouseId:string){return db.prepare("UPDATE inventory_lot_balances SET quantity_milli=quantity_milli-?,value_minor=value_minor-?,version=version+1 WHERE lot_id=? AND warehouse_id=?")
        .bind(quantity, value, lotId, warehouseId);},
prepareLotDepletion(lotId:string,lotId2:string){return db.prepare("UPDATE inventory_lots SET status='depleted' WHERE id=? AND NOT EXISTS(SELECT 1 FROM inventory_lot_balances WHERE lot_id=? AND quantity_milli>0)").bind(lotId, lotId2);},
prepareMaterialLotMove(value:string,lotId:string,warehouseId:string,value2:string,value3:number,value4:number,entryDate:string,userId:string,now:number){return db.prepare("INSERT INTO inventory_lot_moves(id,lot_id,warehouse_id,source_type,source_id,quantity_milli,value_minor,movement_date,created_by,created_at) VALUES(?,?,?,'manufacturing_issue',?,?,?,?,?,?)")
        .bind(value, lotId, warehouseId, value2, value3, value4, entryDate, userId, now);},
prepareOutputLot(lotId:string,companyId:string,productId:string,trackingMode:string,value:string | null,value2:string | null,entryDate:string,value3:string | null,userId:string,now:number){return db.prepare("INSERT INTO inventory_lots(id,company_id,product_id,tracking_mode,lot_code,serial_number,manufactured_date,expiry_date,created_by,created_at) VALUES(?,?,?,?,?,?,?,?,?,?)")
      .bind(lotId, companyId, productId, trackingMode, value, value2, entryDate, value3, userId, now);},
prepareOutputLotStock(lotId:string,warehouseId:string,quantity:number,value:number){return db.prepare("INSERT INTO inventory_lot_balances(lot_id,warehouse_id,quantity_milli,value_minor) VALUES(?,?,?,?)").bind(lotId, warehouseId, quantity, value);},
prepareOutputLotMove(value:string,lotId:string,warehouseId:string,value2:string,quantity:number,value3:number,entryDate:string,userId:string,now:number){return db.prepare("INSERT INTO inventory_lot_moves(id,lot_id,warehouse_id,source_type,source_id,quantity_milli,value_minor,movement_date,created_by,created_at) VALUES(?,?,?,'manufacturing_output',?,?,?,?,?,?)")
        .bind(value, lotId, warehouseId, value2, quantity, value3, entryDate, userId, now);}};}
