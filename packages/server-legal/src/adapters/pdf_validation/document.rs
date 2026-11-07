use der::{Reader, SliceReader};
use lopdf::{Dictionary, Document, LoadOptions, Object, ObjectId};
use std::collections::{BTreeSet, HashSet};

pub(super) struct ParsedPdf {
    pub document: Document,
    pub signatures: Vec<SignatureField>,
}
pub(super) struct SignatureField {
    pub name: String,
    pub dictionary: Dictionary,
    pub field: Dictionary,
}
pub(super) struct SignedContent {
    pub cms: Vec<u8>,
    pub signed: Vec<u8>,
}
pub(super) enum Modification {
    Allowed,
    Forbidden,
    Unresolved,
}

impl ParsedPdf {
    pub fn parse(bytes: &[u8]) -> Result<Self, &'static str> {
        if bytes.len() > 10 * 1024 * 1024 {
            return Err("PDF_SIZE_LIMIT");
        }
        let document = load(bytes)?;
        let mut signatures = Vec::new();
        let mut visited = HashSet::new();
        let catalog = document.catalog().map_err(|_| "INVALID_PDF_CATALOG")?;
        if let Ok(acroform) = catalog.get_deref(b"AcroForm", &document) {
            let acroform = acroform.as_dict().map_err(|_| "INVALID_ACROFORM")?;
            if let Ok(fields) = acroform.get_deref(b"Fields", &document) {
                for field in fields.as_array().map_err(|_| "INVALID_ACROFORM_FIELDS")? {
                    collect_fields(
                        &document,
                        field,
                        "",
                        false,
                        &mut visited,
                        &mut signatures,
                        0,
                    )?;
                }
            }
        }
        Ok(Self {
            document,
            signatures,
        })
    }
}

fn load(bytes: &[u8]) -> Result<Document, &'static str> {
    let document = Document::load_mem_with_options(
        bytes,
        LoadOptions {
            strict: true,
            max_decompressed_size: Some(32 * 1024 * 1024),
            ..Default::default()
        },
    )
    .map_err(|_| "INVALID_PDF")?;
    if document.trailer.has(b"Encrypt") || document.is_encrypted() {
        return Err("ENCRYPTED_PDF");
    }
    if document.xref_recovery.is_some() {
        return Err("REPAIRED_PDF_XREF");
    }
    if document.objects.len() > 100_000 {
        return Err("PDF_OBJECT_LIMIT");
    }
    Ok(document)
}

pub(super) fn resolve<'a>(
    document: &'a Document,
    object: &'a Object,
) -> Result<&'a Object, &'static str> {
    document
        .dereference(object)
        .map(|(_, object)| object)
        .map_err(|_| "INVALID_PDF_REFERENCE")
}

fn collect_fields(
    document: &Document,
    object: &Object,
    parent: &str,
    inherited_sig: bool,
    visited: &mut HashSet<ObjectId>,
    output: &mut Vec<SignatureField>,
    depth: usize,
) -> Result<(), &'static str> {
    if depth > 64 || output.len() > 128 {
        return Err("PDF_FIELD_LIMIT");
    }
    if let Object::Reference(id) = object
        && !visited.insert(*id)
    {
        return Err("CYCLIC_PDF_FIELDS");
    }
    let field = resolve(document, object)?
        .as_dict()
        .map_err(|_| "INVALID_PDF_FIELD")?;
    let part = field
        .get(b"T")
        .ok()
        .and_then(|value| lopdf::decode_text_string(value).ok());
    let name = match part {
        Some(part) if !parent.is_empty() => format!("{parent}.{part}"),
        Some(part) => part,
        None => parent.to_owned(),
    };
    let is_signature = field
        .get(b"FT")
        .ok()
        .and_then(|value| value.as_name().ok())
        .map(|name| name == b"Sig")
        .unwrap_or(inherited_sig);
    if is_signature && let Ok(value) = field.get_deref(b"V", document) {
        let dictionary = value
            .as_dict()
            .map_err(|_| "INVALID_SIGNATURE_DICTIONARY")?;
        output.push(SignatureField {
            name: name.clone(),
            dictionary: dictionary.clone(),
            field: field.clone(),
        });
    }
    if let Ok(kids) = field.get_deref(b"Kids", document) {
        for kid in kids.as_array().map_err(|_| "INVALID_PDF_FIELDS")? {
            collect_fields(
                document,
                kid,
                &name,
                is_signature,
                visited,
                output,
                depth + 1,
            )?;
        }
    }
    Ok(())
}

pub(super) fn byte_range(field: &SignatureField, size: usize) -> Result<[usize; 4], &'static str> {
    let values = field
        .dictionary
        .get(b"ByteRange")
        .and_then(Object::as_array)
        .map_err(|_| "INVALID_BYTE_RANGE")?;
    if values.len() != 4 {
        return Err("INVALID_BYTE_RANGE");
    }
    let mut range = [0; 4];
    for (index, value) in values.iter().enumerate() {
        range[index] = usize::try_from(value.as_i64().map_err(|_| "INVALID_BYTE_RANGE")?)
            .map_err(|_| "INVALID_BYTE_RANGE")?;
    }
    if range[0] != 0
        || range[1] >= range[2]
        || range[2].checked_add(range[3]).is_none_or(|end| end > size)
    {
        return Err("INVALID_BYTE_RANGE");
    }
    Ok(range)
}

pub(super) fn signed_content(
    bytes: &[u8],
    field: &SignatureField,
) -> Result<SignedContent, &'static str> {
    let filter = field
        .dictionary
        .get(b"SubFilter")
        .and_then(Object::as_name)
        .map_err(|_| "UNSUPPORTED_SIGNATURE_SUBFILTER")?;
    if ![b"adbe.pkcs7.detached".as_slice(), b"ETSI.CAdES.detached"].contains(&filter) {
        return Err("UNSUPPORTED_SIGNATURE_SUBFILTER");
    }
    let range = byte_range(field, bytes.len())?;
    let contents = field
        .dictionary
        .get(b"Contents")
        .and_then(Object::as_str)
        .map_err(|_| "INVALID_SIGNATURE_CONTENTS")?;
    let gap = &bytes[range[1]..range[2]];
    if gap.first() != Some(&b'<') || gap.last() != Some(&b'>') {
        return Err("BYTE_RANGE_CONTENTS_MISMATCH");
    }
    // Read the PDF hexadecimal string, not any arbitrary unsigned gap.
    let hex_bytes: Vec<u8> = gap[1..gap.len() - 1]
        .iter()
        .copied()
        .filter(|byte| !byte.is_ascii_whitespace())
        .collect();
    if !hex_bytes.len().is_multiple_of(2)
        || hex::decode(hex_bytes).map_err(|_| "INVALID_SIGNATURE_CONTENTS")? != contents
    {
        return Err("BYTE_RANGE_CONTENTS_MISMATCH");
    }
    let mut reader = SliceReader::new(contents).map_err(|_| "INVALID_SIGNATURE_CMS")?;
    let cms = reader
        .tlv_bytes()
        .map_err(|_| "INVALID_SIGNATURE_CMS")?
        .to_vec();
    if contents[cms.len()..].iter().any(|byte| *byte != 0) {
        return Err("INVALID_SIGNATURE_PADDING");
    }
    let mut signed = Vec::with_capacity(range[1] + range[3]);
    signed.extend_from_slice(&bytes[..range[1]]);
    signed.extend_from_slice(&bytes[range[2]..range[2] + range[3]]);
    Ok(SignedContent { cms, signed })
}

fn changed_keys(left: &Dictionary, right: &Dictionary) -> BTreeSet<Vec<u8>> {
    left.iter()
        .map(|(key, _)| key.clone())
        .chain(right.iter().map(|(key, _)| key.clone()))
        .filter(|key| left.get(key).ok() != right.get(key).ok())
        .collect()
}

fn only_keys(changes: &BTreeSet<Vec<u8>>, allowed: &[&[u8]]) -> bool {
    changes.iter().all(|key| allowed.contains(&key.as_slice()))
}

// New appearance/DSS resources may refer to unchanged old resources, but never
// grant permission to redefine arbitrary pre-existing document objects.
fn allow_new_graph(
    object: &Object,
    old: &Document,
    current: &Document,
    allowed: &mut HashSet<ObjectId>,
    depth: usize,
) -> bool {
    if depth > 64 {
        return false;
    }
    let value = if let Object::Reference(id) = object {
        if old.objects.contains_key(id) {
            return allowed.contains(id) || old.objects.get(id) == current.objects.get(id);
        }
        if !allowed.insert(*id) {
            return true;
        }
        match current.objects.get(id) {
            Some(value) => value,
            None => return false,
        }
    } else {
        object
    };
    match value {
        Object::Dictionary(dictionary) => dictionary
            .iter()
            .all(|(_, value)| allow_new_graph(value, old, current, allowed, depth + 1)),
        Object::Stream(stream) => stream
            .dict
            .iter()
            .all(|(_, value)| allow_new_graph(value, old, current, allowed, depth + 1)),
        Object::Array(values) => values
            .iter()
            .all(|value| allow_new_graph(value, old, current, allowed, depth + 1)),
        _ => true,
    }
}

struct FieldLock {
    action: Vec<u8>,
    fields: Vec<String>,
}

impl FieldLock {
    fn parse(parameters: &Dictionary, document: &Document) -> Option<Self> {
        let action = parameters.get(b"Action").ok()?.as_name().ok()?.to_vec();
        if ![b"All".as_slice(), b"Include", b"Exclude"].contains(&action.as_slice()) {
            return None;
        }
        let fields = if action == b"All" {
            Vec::new()
        } else {
            parameters
                .get_deref(b"Fields", document)
                .ok()?
                .as_array()
                .ok()?
                .iter()
                .map(|value| lopdf::decode_text_string(value).ok())
                .collect::<Option<Vec<_>>>()?
        };
        Some(Self { action, fields })
    }
    fn locks(&self, field: &str) -> bool {
        if self.action == b"All" {
            return true;
        }
        let selected = self
            .fields
            .iter()
            .any(|name| field == name || field.starts_with(&format!("{name}.")));
        if self.action == b"Include" {
            selected
        } else {
            !selected
        }
    }
}

fn field_name(document: &Document, id: ObjectId) -> Option<String> {
    let mut parts = Vec::new();
    let mut seen = HashSet::new();
    let mut current = id;
    loop {
        if !seen.insert(current) || seen.len() > 64 {
            return None;
        }
        let dictionary = document.get_dictionary(current).ok()?;
        if let Ok(name) = dictionary.get(b"T") {
            parts.push(lopdf::decode_text_string(name).ok()?);
        }
        if let Ok(parent) = dictionary.get(b"Parent") {
            current = parent.as_reference().ok()?;
        } else {
            break;
        }
    }
    parts.reverse();
    Some(parts.join("."))
}

fn references(
    document: &Document,
    object: &Object,
    visited: &mut HashSet<ObjectId>,
    depth: usize,
) -> bool {
    if depth > 128 {
        return false;
    }
    let value = if let Object::Reference(id) = object {
        if !visited.insert(*id) {
            return true;
        }
        let Some(value) = document.objects.get(id) else {
            return false;
        };
        value
    } else {
        object
    };
    match value {
        Object::Dictionary(dictionary) => dictionary
            .iter()
            .all(|(_, value)| references(document, value, visited, depth + 1)),
        Object::Stream(stream) => stream
            .dict
            .iter()
            .all(|(_, value)| references(document, value, visited, depth + 1)),
        Object::Array(values) => values
            .iter()
            .all(|value| references(document, value, visited, depth + 1)),
        _ => true,
    }
}

fn allow_info_update(old: &Document, current: &Document, allowed: &mut HashSet<ObjectId>) -> bool {
    let Some(Object::Reference(id)) = current.trailer.get(b"Info").ok() else {
        return true;
    };
    let Some(Object::Dictionary(info)) = current.objects.get(id) else {
        return false;
    };
    if !info.iter().all(|(_, value)| {
        matches!(
            value,
            Object::Null
                | Object::Boolean(_)
                | Object::Integer(_)
                | Object::Real(_)
                | Object::Name(_)
                | Object::String(..)
        )
    }) {
        return false;
    }
    let mut drawing_graph = HashSet::new();
    let Ok(root) = old.trailer.get(b"Root") else {
        return false;
    };
    if !references(old, root, &mut drawing_graph, 0) || drawing_graph.contains(id) {
        return false;
    }
    // An Info update cannot acquire a drawing/resource object's identity.
    if old.objects.contains_key(id)
        && old.trailer.get(b"Info").ok() != Some(&Object::Reference(*id))
    {
        return false;
    }
    allowed.insert(*id);
    true
}

fn valid_der_stream_array(document: &Document, value: &Object) -> bool {
    resolve(document, value)
        .ok()
        .and_then(|value| value.as_array().ok())
        .is_some_and(|values| {
            values.iter().all(|value| {
                matches!(value, Object::Reference(_))
                    && matches!(resolve(document, value), Ok(Object::Stream(_)))
            })
        })
}

fn valid_dss(document: &Document, value: &Object) -> bool {
    let Some(dictionary) = resolve(document, value)
        .ok()
        .and_then(|value| value.as_dict().ok())
    else {
        return false;
    };
    if !dictionary.iter().all(|(key, _)| {
        [b"Type".as_slice(), b"VRI", b"Certs", b"CRLs", b"OCSPs"].contains(&key.as_slice())
    }) {
        return false;
    }
    for key in [b"Certs".as_slice(), b"CRLs", b"OCSPs"] {
        if let Ok(value) = dictionary.get(key)
            && !valid_der_stream_array(document, value)
        {
            return false;
        }
    }
    if let Ok(vri) = dictionary.get_deref(b"VRI", document) {
        let Ok(vri) = vri.as_dict() else {
            return false;
        };
        for (key, value) in vri {
            if key.len() != 40
                || !key
                    .iter()
                    .all(|byte| byte.is_ascii_digit() || (b'A'..=b'F').contains(byte))
            {
                return false;
            }
            let Some(value) = resolve(document, value)
                .ok()
                .and_then(|value| value.as_dict().ok())
            else {
                return false;
            };
            if !value.iter().all(|(key, _)| {
                [b"Type".as_slice(), b"TU", b"TS", b"Cert", b"CRL", b"OCSP"]
                    .contains(&key.as_slice())
            }) {
                return false;
            }
            for key in [b"Cert".as_slice(), b"CRL", b"OCSP"] {
                if let Ok(value) = value.get(key)
                    && !valid_der_stream_array(document, value)
                {
                    return false;
                }
            }
            if let Ok(timestamp) = value.get(b"TS")
                && (!matches!(timestamp, Object::Reference(_))
                    || !matches!(resolve(document, timestamp), Ok(Object::Stream(_))))
            {
                return false;
            }
        }
    }
    true
}

/// A maintenance graph may update its own existing nodes only when no other
/// catalog branch can reach them, in either revision. This prevents a DSS or
/// Metadata alias from granting permission to overwrite a drawing object.
fn allow_owned_maintenance_graph(
    key: &[u8],
    old: &Document,
    current: &Document,
    allowed: &mut HashSet<ObjectId>,
) -> Result<(), Modification> {
    let (Ok(old_catalog), Ok(new_catalog)) = (old.catalog(), current.catalog()) else {
        return Err(Modification::Unresolved);
    };
    let Some(new_value) = new_catalog.get(key).ok() else {
        return Ok(());
    };
    let mut old_graph = HashSet::new();
    if let Ok(value) = old_catalog.get(key)
        && !references(old, value, &mut old_graph, 0)
    {
        return Err(Modification::Unresolved);
    }
    let mut new_graph = HashSet::new();
    if !references(current, new_value, &mut new_graph, 0) {
        return Err(Modification::Unresolved);
    }
    let mut old_protected_catalog = old_catalog.clone();
    old_protected_catalog.remove(key);
    let mut new_protected_catalog = new_catalog.clone();
    new_protected_catalog.remove(key);
    let mut old_protected = HashSet::new();
    let mut new_protected = HashSet::new();
    if !references(
        old,
        &Object::Dictionary(old_protected_catalog),
        &mut old_protected,
        0,
    ) || !references(
        current,
        &Object::Dictionary(new_protected_catalog),
        &mut new_protected,
        0,
    ) {
        return Err(Modification::Unresolved);
    }
    for id in &new_graph {
        if old.objects.contains_key(id)
            && old.objects.get(id) != current.objects.get(id)
            && (old_protected.contains(id) || new_protected.contains(id))
        {
            return Err(Modification::Forbidden);
        }
    }
    for id in old_graph.intersection(&new_graph) {
        if old.objects.get(id) != current.objects.get(id) {
            if old_protected.contains(id) || new_protected.contains(id) {
                return Err(Modification::Forbidden);
            }
            allowed.insert(*id);
        }
    }
    if allow_new_graph(new_value, old, current, allowed, 0)
        && new_graph
            .iter()
            .all(|id| allow_new_graph(&Object::Reference(*id), old, current, allowed, 0))
    {
        Ok(())
    } else {
        Err(Modification::Unresolved)
    }
}

fn valid_xmp(document: &Document, value: &Object) -> bool {
    if !matches!(value, Object::Reference(_)) {
        return false;
    }
    let Ok(Object::Stream(stream)) = resolve(document, value) else {
        return false;
    };
    let Ok(bytes) = stream.get_plain_content_with_limit(32 * 1024 * 1024) else {
        return false;
    };
    let Ok(text) = std::str::from_utf8(&bytes) else {
        return false;
    };
    let options = roxmltree::ParsingOptions {
        allow_dtd: false,
        nodes_limit: 100_000,
        entity_resolver: None,
    };
    let Ok(xml) = roxmltree::Document::parse_with_options(text, options) else {
        return false;
    };
    let root = xml.root_element();
    root.tag_name().namespace() == Some("adobe:ns:meta/") && root.tag_name().name() == "xmpmeta"
}

fn allow_added_signatures(
    old: &Document,
    current: &Document,
    allowed: &mut HashSet<ObjectId>,
    permission: i64,
) -> Result<(), Modification> {
    let old_catalog = old.catalog().map_err(|_| Modification::Unresolved)?;
    let new_catalog = current.catalog().map_err(|_| Modification::Unresolved)?;
    let old_form = old_catalog
        .get_deref(b"AcroForm", old)
        .and_then(Object::as_dict)
        .map_err(|_| Modification::Unresolved)?;
    let new_form = new_catalog
        .get_deref(b"AcroForm", current)
        .and_then(Object::as_dict)
        .map_err(|_| Modification::Unresolved)?;
    if !only_keys(&changed_keys(old_form, new_form), &[b"Fields"]) {
        return Err(Modification::Unresolved);
    }
    let old_fields = old_form
        .get_deref(b"Fields", old)
        .and_then(Object::as_array)
        .map_err(|_| Modification::Unresolved)?;
    let new_fields = new_form
        .get_deref(b"Fields", current)
        .and_then(Object::as_array)
        .map_err(|_| Modification::Unresolved)?;
    if !old_fields.iter().all(|field| new_fields.contains(field)) {
        return Err(Modification::Forbidden);
    }
    let added: Vec<_> = new_fields
        .iter()
        .filter(|field| !old_fields.contains(field))
        .collect();
    let mut added_ids = HashSet::new();
    for field in &added {
        let dictionary = resolve(current, field)
            .and_then(|value| value.as_dict().map_err(|_| "INVALID_FIELD"))
            .map_err(|_| Modification::Unresolved)?;
        if !matches!(dictionary.get(b"FT").and_then(Object::as_name), Ok(b"Sig")) {
            return Err(Modification::Forbidden);
        }
        let value = dictionary
            .get_deref(b"V", current)
            .and_then(Object::as_dict)
            .map_err(|_| Modification::Unresolved)?;
        if matches!(
            value.get(b"Type").and_then(Object::as_name),
            Ok(b"DocTimeStamp")
        ) {
            return Err(Modification::Unresolved);
        }
        if permission == 1 {
            return Err(Modification::Forbidden);
        }
        added_ids.insert(field.as_reference().map_err(|_| Modification::Unresolved)?);
    }
    for (page, old_id) in old.get_pages() {
        if current.get_pages().get(&page) != Some(&old_id) {
            return Err(Modification::Forbidden);
        }
        let before = old
            .get_dictionary(old_id)
            .map_err(|_| Modification::Unresolved)?;
        let after = current
            .get_dictionary(old_id)
            .map_err(|_| Modification::Unresolved)?;
        if !only_keys(&changed_keys(before, after), &[b"Annots"]) {
            return Err(Modification::Forbidden);
        }
        let before_annots = before
            .get_deref(b"Annots", old)
            .ok()
            .and_then(|value| value.as_array().ok())
            .cloned()
            .unwrap_or_default();
        let after_annots = after
            .get_deref(b"Annots", current)
            .ok()
            .and_then(|value| value.as_array().ok())
            .cloned()
            .unwrap_or_default();
        if !before_annots
            .iter()
            .all(|annot| after_annots.contains(annot))
        {
            return Err(Modification::Forbidden);
        }
        if !after_annots
            .iter()
            .filter(|annot| !before_annots.contains(annot))
            .all(|annot| annot.as_reference().is_ok_and(|id| added_ids.contains(&id)))
        {
            return Err(Modification::Forbidden);
        }
        allowed.insert(old_id);
        if let Ok(Object::Reference(id)) = after.get(b"Annots") {
            allowed.insert(*id);
        }
    }
    if let Ok(Object::Reference(id)) = new_catalog.get(b"AcroForm") {
        allowed.insert(*id);
    }
    if let Ok(Object::Reference(id)) = new_form.get(b"Fields") {
        allowed.insert(*id);
    }
    for field in added {
        if !allow_new_graph(field, old, current, allowed, 0) {
            return Err(Modification::Unresolved);
        }
    }
    Ok(())
}

pub(super) fn modification_policy(
    bytes: &[u8],
    current: &ParsedPdf,
    signature: &SignatureField,
) -> Modification {
    let Ok(range) = byte_range(signature, bytes.len()) else {
        return Modification::Unresolved;
    };
    let end = range[2] + range[3];
    if end == bytes.len() {
        return Modification::Allowed;
    }
    let Ok(original) = ParsedPdf::parse(&bytes[..end]) else {
        return Modification::Unresolved;
    };
    let old = original.document;
    let original_signatures: Vec<_> = original
        .signatures
        .iter()
        .filter(|field| field.name == signature.name)
        .collect();
    if original_signatures.len() != 1 {
        return Modification::Unresolved;
    }
    let original_signature = original_signatures[0];
    // Modification permissions must themselves be inside the signed revision.
    // A later redefinition of /Reference or /Contents cannot loosen them.
    if original_signature.dictionary != signature.dictionary {
        return Modification::Forbidden;
    }
    let Ok(old_catalog) = old.catalog() else {
        return Modification::Unresolved;
    };
    let Ok(new_catalog) = current.document.catalog() else {
        return Modification::Unresolved;
    };
    if old.trailer.get(b"Root").ok() != current.document.trailer.get(b"Root").ok() {
        return Modification::Forbidden;
    }
    let mut permission = 2;
    let mut field_policies = Vec::new();
    if let Ok(references) = signature
        .dictionary
        .get(b"Reference")
        .and_then(Object::as_array)
    {
        for reference in references {
            let Ok(Object::Dictionary(reference)) = resolve(&old, reference) else {
                return Modification::Unresolved;
            };
            match reference.get(b"TransformMethod").and_then(Object::as_name) {
                Ok(b"DocMDP") => {
                    let Ok(params) = reference
                        .get_deref(b"TransformParams", &old)
                        .and_then(Object::as_dict)
                    else {
                        return Modification::Unresolved;
                    };
                    permission = params.get(b"P").and_then(Object::as_i64).unwrap_or(2);
                    if !(1..=3).contains(&permission) {
                        return Modification::Unresolved;
                    }
                }
                Ok(b"FieldMDP") => {
                    let Ok(params) = reference
                        .get_deref(b"TransformParams", &old)
                        .and_then(Object::as_dict)
                    else {
                        return Modification::Unresolved;
                    };
                    let Some(policy) = FieldLock::parse(params, &old) else {
                        return Modification::Unresolved;
                    };
                    field_policies.push(policy);
                }
                _ => return Modification::Unresolved,
            }
        }
    }
    if let Ok(lock) = original_signature
        .field
        .get_deref(b"Lock", &old)
        .and_then(Object::as_dict)
    {
        let Some(policy) = FieldLock::parse(lock, &old) else {
            return Modification::Unresolved;
        };
        field_policies.push(policy);
    }
    let mut allowed = HashSet::new();
    let root_id = old.trailer.get(b"Root").and_then(Object::as_reference).ok();
    if let Some(id) = root_id {
        allowed.insert(id);
    }
    if !allow_info_update(&old, &current.document, &mut allowed) {
        return Modification::Unresolved;
    }
    let catalog_changes = changed_keys(old_catalog, new_catalog);
    // Signature-maintenance DSS updates are allowed even for DocMDP P=1.
    if !only_keys(&catalog_changes, &[b"DSS", b"AcroForm", b"Metadata"]) {
        return if permission == 1 {
            Modification::Forbidden
        } else {
            Modification::Unresolved
        };
    }
    if let Err(state) = allow_added_signatures(&old, &current.document, &mut allowed, permission) {
        return state;
    }
    if let Ok(dss) = new_catalog.get(b"DSS") {
        if !valid_dss(&current.document, dss) {
            return Modification::Forbidden;
        }
        if let Err(state) =
            allow_owned_maintenance_graph(b"DSS", &old, &current.document, &mut allowed)
        {
            return state;
        }
    }
    if let Ok(metadata) = new_catalog.get(b"Metadata") {
        if !valid_xmp(&current.document, metadata)
            || old_catalog
                .get(b"Metadata")
                .is_ok_and(|value| !valid_xmp(&old, value))
        {
            return Modification::Forbidden;
        }
        if let Err(state) =
            allow_owned_maintenance_graph(b"Metadata", &old, &current.document, &mut allowed)
        {
            return state;
        }
    }
    for (id, old_object) in &old.objects {
        let Some(new_object) = current.document.objects.get(id) else {
            return Modification::Forbidden;
        };
        if old_object == new_object || allowed.contains(id) {
            continue;
        }
        if let (Object::Dictionary(before), Object::Dictionary(after)) = (old_object, new_object)
            && before.get(b"FT").is_ok()
            && only_keys(&changed_keys(before, after), &[b"V", b"AS", b"AP"])
        {
            if permission == 1 {
                return Modification::Forbidden;
            }
            let Some(name) = field_name(&old, *id) else {
                return Modification::Unresolved;
            };
            if field_policies.iter().any(|policy| policy.locks(&name)) {
                return Modification::Forbidden;
            }
            for key in [b"AP".as_slice(), b"V"] {
                if let Ok(value) = after.get(key)
                    && !allow_new_graph(value, &old, &current.document, &mut allowed, 0)
                {
                    return Modification::Unresolved;
                }
            }
            allowed.insert(*id);
            continue;
        }
        let container = match new_object {
            Object::Stream(stream) => stream.dict.get(b"Type").and_then(Object::as_name).ok(),
            _ => None,
        };
        if matches!(container, Some(b"XRef" | b"ObjStm")) {
            return Modification::Forbidden;
        }
        // A changed page drawing stream is a proven content modification.
        if let Object::Stream(_) = old_object
            && old
                .get_pages()
                .values()
                .any(|page| old.get_page_contents(*page).contains(id))
        {
            return Modification::Forbidden;
        }
        return Modification::Unresolved;
    }
    for (id, object) in &current.document.objects {
        if old.objects.contains_key(id) || allowed.contains(id) {
            continue;
        }
        if matches!(object, Object::Stream(stream) if matches!(stream.dict.get(b"Type").and_then(Object::as_name), Ok(b"XRef" | b"ObjStm")))
        {
            continue;
        }
        return Modification::Unresolved;
    }
    Modification::Allowed
}
