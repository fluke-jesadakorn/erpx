use axum::http::HeaderMap;
use erpx_server_shared::public::BusinessError;
/// Scope selectors only, never principal/authority facts. The root gateway
/// adapter must supply actual branch/department requested path selectors.
pub trait BusinessScopeSlugs: Send + Sync + 'static {
    fn slugs(&self, headers: &HeaderMap)
    -> Result<(Option<String>, Option<String>), BusinessError>;
}

/// A declared Company target hint is never an authorization grant. The chosen
/// owner still resolves the original selected session/CAS and current Company
/// in its native unit. Preserve unambiguous single-target legacy reads only.
#[cfg(any(feature="existing-business",feature="finance-field-read"))]
pub(crate) fn selected_company_target<'a,T>(
    targets:&'a std::collections::BTreeMap<(String,String,String),T>,
    host:&str,holding:&str,headers:&HeaderMap,
)->Result<&'a T,BusinessError>{
    use erpx_server_identity_access::public::account_inbound_http::unique_request_header;
    use erpx_server_shared::public::FailureKind;
    let missing=||BusinessError::new(FailureKind::NotFound,"NOT_FOUND","Not found.");
    let requested=unique_request_header(headers,"x-erpx-context-company")?;
    if let Some(target)=requested.and_then(|company|targets.get(&(host.into(),holding.into(),company.into()))){ return Ok(target); }
    let mut candidates=targets.iter().filter(|((h,s,_),_)|h==host&&s==holding);
    let (_,target)=candidates.next().ok_or_else(missing)?;
    if candidates.next().is_some(){
        return Err(if requested.is_some(){
            BusinessError::new(FailureKind::Conflict,"WORK_CONTEXT_CHANGED","Your work role changed. Reload and review before submitting.")
        }else{
            BusinessError::new(FailureKind::Conflict,"WORK_COMPANY_REQUIRED","Select the Company before reading this Holding workspace.")
        });
    }
    // Preserve the existing single-target authorization/CAS error precedence:
    // the owner rejects a mismatched Company before parsing business input.
    Ok(target)
}
