import 'server-only';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import type {CommercialStorageRow as StorageRow} from "@erpx/server-commercial/public/usage-contract";
export function commercialStoredOriginals(db:SqlDatabase,start:number,end:number,branchId:string|null){return db.prepare(`SELECT l.id,l.company_id,s.branch_id,l.bytes,l.created_at,l.destroyed_at
      FROM attachment_storage_ledger l
      JOIN attachments a ON a.id=l.attachment_id
      LEFT JOIN upload_intents u ON u.id=a.upload_id
      LEFT JOIN attachment_sets s ON s.id=u.attachment_set_id AND s.company_id=l.company_id
      WHERE l.variant='original' AND l.state IN ('stored','destroyed')
        AND l.created_at<? AND (l.destroyed_at IS NULL OR l.destroyed_at>?)
        AND (?::text IS NULL OR s.branch_id=?)
      ORDER BY l.created_at DESC,l.id DESC`).bind(end, start, branchId, branchId).all<StorageRow>();}
export function commercialUnreconciledOriginals(db:SqlDatabase,end:number,branchId:string|null){return db.prepare(`SELECT COUNT(*)::int AS count FROM attachments a
      LEFT JOIN upload_intents u ON u.id=a.upload_id
      LEFT JOIN attachment_sets s ON s.id=u.attachment_set_id AND s.company_id=a.company_id
      WHERE a.source_state<>'destroyed' AND a.created_at<?
        AND NOT EXISTS(SELECT 1 FROM attachment_storage_ledger l WHERE l.attachment_id=a.id AND l.variant='original')
        AND (?::text IS NULL OR s.branch_id=?)`).bind(end, branchId, branchId).first<{count:number}>();}
export function commercialFileReconciliation(db:SqlDatabase,){return db.prepare("SELECT ready FROM attachment_reconciliation_runs ORDER BY created_at DESC LIMIT 1").first<{ready:boolean}>();}
export function commercialEarliestOriginal(db:SqlDatabase,){return db.prepare("SELECT MIN(created_at) earliest FROM attachment_storage_ledger WHERE variant='original'")
          .first<{ earliest: number | null }>();}
