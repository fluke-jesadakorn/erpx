//! Pure Files target decision. The caller must first validate the attachment
//! registry entry, current assignment and selected-Company Quality witness, then
//! separately enforce Quality's purchase policy and retain all source guards.
#[derive(Debug, Clone, PartialEq)]
pub struct QualityHoldDescriptor<'a> {
    pub company_id: &'a str,
    pub branch_id: Option<&'a str>,
    pub version: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct QualityHoldScope<'a> {
    pub company_id: &'a str,
    pub branch_id: Option<&'a str>,
    pub department_id: Option<&'a str>,
    pub version: f64,
    pub editable: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QualityHoldPolicyError {
    pub status: u16,
    pub code: &'static str,
    pub message: &'static str,
}

/// `purpose` comes from the registered Files policy. All operations other than
/// `read` follow the TypeScript write branch. Permissions are the *current*
/// effective intersection, not claims supplied by a client.
pub fn quality_hold_required_permission(purpose: &str, read: bool) -> &'static str {
    if read { "quality.read" } else {
        match purpose {
            "quality_inspection_evidence" | "inspection_evidence" => "quality.inspect",
            "disposition_evidence" => "quality.release",
            _ => "quality.hold",
        }
    }
}

pub fn quality_hold_target<'a>(
    row: Option<&QualityHoldDescriptor<'a>>,
    actor_branch: Option<&str>,
    allowed: bool,
    read: bool,
) -> Result<QualityHoldScope<'a>, QualityHoldPolicyError> {
    let Some(row) = row else {
        return Err(QualityHoldPolicyError { status: 404, code: "ATTACHMENT_TARGET_NOT_FOUND", message: "Quality hold not found or unavailable." });
    };
    if !allowed {
        return Err(QualityHoldPolicyError { status: 403, code: "ATTACHMENT_TARGET_NOT_FOUND", message: "Quality hold not found or unavailable." });
    }
    if actor_branch.is_some_and(|branch| !branch.is_empty() && Some(branch) != row.branch_id) {
        return Err(QualityHoldPolicyError { status: 403, code: "ATTACHMENT_SCOPE_FORBIDDEN", message: "The quality hold is outside your Branch scope." });
    }
    Ok(QualityHoldScope { company_id: row.company_id, branch_id: row.branch_id, department_id: None, version: row.version as f64, editable: !read })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    #[test]
    fn quality_target_matches_installed_typescript_policy() {
        let cases = json!([
            {"target":"quality_hold","purpose":"defect_photo","operation":"read","permissions":["quality.read"],"branch":null,"row":{"company_id":"c","branch_id":"b","department_id":null,"version":3}},
            {"target":"quality_hold","purpose":"quality_inspection_evidence","operation":"write","permissions":["quality.inspect"],"branch":"b","row":{"company_id":"c","branch_id":"b","department_id":null,"version":7}},
            {"target":"inventory_quality_hold","purpose":"inspection_evidence","operation":"bind","permissions":["quality.inspect"],"branch":"","row":{"company_id":"c","branch_id":null,"department_id":null,"version":9}},
            {"target":"quality_hold","purpose":"disposition_evidence","operation":"publish","permissions":["quality.release"],"branch":"b","row":{"company_id":"c","branch_id":null,"department_id":null,"version":1}},
            {"target":"inventory_quality_hold","purpose":"hold_evidence","operation":"withdraw","permissions":["quality.hold"],"branch":null,"row":{"company_id":"c","branch_id":"b","department_id":null,"version":1}},
            {"target":"quality_hold","purpose":"defect_photo","operation":"write","permissions":[],"branch":"x","row":{"company_id":"c","branch_id":"b","department_id":null,"version":1}},
            {"target":"quality_hold","purpose":"defect_photo","operation":"write","permissions":[],"branch":null,"row":null},
            {"target":"quality_hold","purpose":"defect_photo","operation":"read","permissions":["quality.read"],"branch":null,"row":{"company_id":"c","branch_id":null,"department_id":null,"version":9007199254740991_i64}},
            {"target":"quality_hold","purpose":"defect_photo","operation":"read","permissions":["quality.read"],"branch":null,"row":{"company_id":"c","branch_id":null,"department_id":null,"version":9007199254740993_i64}},
            {"target":"quality_hold","purpose":"defect_photo","operation":"read","permissions":["quality.read"],"branch":null,"row":{"company_id":"c","branch_id":null,"department_id":null,"version":i64::MAX}}
        ]);
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let application = root.join("packages/server-files/src/application/attachments/policy.ts");
        let script = r#"if(Bun.version!=='1.4.3')throw Error('PINNED_BUN_REQUIRED');const {createAttachmentPolicy}=await import(process.argv[1]);const cases=JSON.parse(process.argv[2]);const out=[];for(const c of cases){const api=createAttachmentPolicy({clock:{now:()=>1},error:(status,code,message)=>Object.assign(new Error(message),{status,code})});const db={readCurrentAssignment:async()=>({permissions:JSON.stringify(c.permissions),modules_json:'[]',role_permissions:'[]'}),authorizePortalCollaborationAttachmentTarget:async()=>null,authorizeSensitiveAttachmentTarget:async()=>null,authorizeFinanceOperationsAttachmentTarget:async()=>null,authorizeRemainingOperationalAttachmentTarget:async()=>null,readQualityHold:async()=>c.row};const actor={userId:'u',companyId:'c',assignmentId:'a',role:'custom_test',permissions:c.permissions,modules:[],branchId:c.branch,departmentId:null};try{const policy=api.attachmentPolicy(c.target,c.purpose,'action');out.push({scope:await api.authorizeAttachmentTarget(db,actor,policy,'hold',c.operation)})}catch(e){out.push({error:{status:e.status,code:e.code,message:e.message}})}}process.stdout.write(JSON.stringify(out));"#;
        let output = std::process::Command::new(std::env::var("ERPX_BUN_BIN").expect("pinned Bun required"))
            .current_dir(root).args(["--conditions=react-server", "-e", script]).arg(application).arg(cases.to_string()).output().unwrap();
        assert!(output.status.success(), "TS oracle: {}", String::from_utf8_lossy(&output.stderr));
        let mut expected: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
        // Normalize both JSON numbers as IEEE-754 values; comparing JSON's
        // integer representation would obscure JavaScript Number rounding.
        for item in &mut expected {
            if let Some(scope) = item.get_mut("scope") {
                let number = scope["version"].as_f64().unwrap();
                scope["version"] = json!(number);
            }
        }
        let actual: Vec<Value> = cases.as_array().unwrap().iter().map(|case| {
            let value = &case["row"];
            let row = (!value.is_null()).then(|| QualityHoldDescriptor { company_id: value["company_id"].as_str().unwrap(), branch_id: value["branch_id"].as_str(), version: value["version"].as_i64().unwrap() });
            let read = case["operation"] == "read";
            let permission = quality_hold_required_permission(case["purpose"].as_str().unwrap(), read);
            let allowed = case["permissions"].as_array().unwrap().iter().any(|p| p == permission);
            match quality_hold_target(row.as_ref(), case["branch"].as_str(), allowed, read) {
                Ok(s) => json!({"scope":{"companyId":s.company_id,"branchId":s.branch_id,"departmentId":s.department_id,"version":s.version,"editable":s.editable}}),
                Err(e) => json!({"error":{"status":e.status,"code":e.code,"message":e.message}}),
            }
        }).collect();
        assert_eq!(actual, expected);
    }
}
