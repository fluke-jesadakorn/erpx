import "server-only";
import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
export function peopleEvaluationFilesFacts(database: SqlDatabase) {
    return { readEvaluationDetailFiles(companyId: string, assessmentId: string) { return database.prepare("SELECT l.id,a.original_name filename FROM attachment_links l JOIN attachment_sets s ON s.id=l.set_id JOIN attachments a ON a.id=l.attachment_id WHERE s.company_id=? AND s.target_type='hr_evaluation_assessment' AND s.target_id=? AND l.state='bound' AND a.source_state='available'").bind(companyId, assessmentId).all<{
            id: string;
            filename: string;
        }>(); },
        readAddEvaluationEvidenceLink(sourceId: string | null, assessmentId: string, companyId: string) { return database.prepare("SELECT l.id FROM attachment_links l JOIN attachment_sets s ON s.id=l.set_id JOIN attachments f ON f.id=l.attachment_id WHERE l.id=? AND s.target_type='hr_evaluation_assessment' AND s.target_id=? AND s.company_id=? AND l.state='bound' AND f.source_state='available'").bind(sourceId, assessmentId, companyId).first(); } };
}
