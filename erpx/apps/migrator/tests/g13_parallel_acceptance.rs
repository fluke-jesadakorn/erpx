//! Independent G13 deployment/recovery acceptance checks.
//!
//! This packet owns no deployment implementation.  It reads the checked-in
//! migration, protobuf, worker, server, and Folio bridge contracts and keeps
//! disposable runtime work explicitly queued.  In particular, it never opens
//! a database connection, starts a worker, invokes a process, probes a
//! service, or changes command ownership.

use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs,
    path::{Path, PathBuf},
};

use prost::Message;
use prost_types::{FileDescriptorProto, FileDescriptorSet};

const MIGRATION_LIB: &str = include_str!("../../../crates/migration/src/lib.rs");
const CONTROL_MIGRATION: &str =
    include_str!("../../../crates/migration/migrations/0001_control_plane.sql");
const G13_CONTROL_MIGRATION: &str =
    include_str!("../../../crates/migration/migrations/0058_migration_execution.sql");
const G13_SOURCE_SNAPSHOT_MIGRATION: &str =
    include_str!("../../../crates/migration/migrations/0059_migration_source_snapshots.sql");
const G13_READ_MIGRATION: &str =
    include_str!("../../../crates/migration/migrations_read/0011_migration_status.sql");
const G13_SOURCE_SNAPSHOT_READ_MIGRATION: &str = include_str!(
    "../../../crates/migration/migrations_read/0012_migration_source_snapshot_status.sql"
);
const COMMAND_RECEIPTS_MIGRATION: &str =
    include_str!("../../../crates/migration/migrations/0002_g1_002_command_receipts.sql");
const PROTO_BUILD: &str = include_str!("../../../crates/proto/build.rs");
const PROTO_LIB: &str = include_str!("../../../crates/proto/src/lib.rs");
const COMMON_PROTO: &str = include_str!("../../../crates/proto/proto/erpx/common/v1/context.proto");
const PLATFORM_PROTO: &str =
    include_str!("../../../crates/proto/proto/erpx/platform/v1/platform.proto");
const CONTRACT_POLICY: &str = include_str!("../../../CONTRACT_POLICY.md");
const MIGRATOR_MAIN: &str = include_str!("../src/main.rs");
const WORKER_LIB: &str = include_str!("../../worker/src/lib.rs");
const WORKER_MAIN: &str = include_str!("../../worker/src/main.rs");
const G13_WORKER_RUNTIME: &str = include_str!("../../worker/src/g13_migration_runtime.rs");
const G13_FOLIO_BRIDGE: &str = include_str!("../../worker/src/folio_bridge.rs");
const PROJECTION_SCHEDULER: &str =
    include_str!("../../../crates/infrastructure/src/tenant_projection_scheduler.rs");
const SERVER_MAIN: &str = include_str!("../../server/src/main.rs");
const G13_SERVER: &str = include_str!("../../server/src/migration.rs");
const CONSOLE_MAIN: &str = include_str!("../../console/src/main.rs");
const CONSOLE_PRODUCT_COMPLETION: &str = include_str!("../../console/src/product_completion.rs");
const G13_DISPOSABLE_ACCEPTANCE: &str =
    include_str!("../../../crates/infrastructure/tests/g13_disposable_acceptance.rs");
const FOLIO_PLATFORM_PAGE: &str =
    include_str!("../../../../app/(app)/(protected)/platform/page.tsx");
const FOLIO_PLATFORM_CONSOLE: &str =
    include_str!("../../../../components/platform/PlatformConsole.tsx");
const FOLIO_PLATFORM_ACCESS: &str = include_str!("../../../../lib/platform/access.ts");
const FOLIO_PLATFORM_QUERIES: &str = include_str!("../../../../lib/platform/queries.ts");
const FOLIO_CONTROL_MIGRATION: &str =
    include_str!("../../../../db/migrations/2026-07-23-erpx-control-plane.sql");

const G13_DATABASE_REFERENCES: [&str; 3] = [
    "ERPX_G13_SOURCE_DATABASE_URL",
    "ERPX_G13_TARGET_DATABASE_URL",
    "ERPX_G13_ROLLBACK_DATABASE_URL",
];
const G13_RUNTIME_DATABASE_REFERENCES: [&str; 4] = [
    "ERPX_CONTROL_WRITE_DATABASE_URL",
    "ERPX_CONTROL_READ_DATABASE_URL",
    "ERPX_TENANT_ADMIN_DATABASE_URL",
    "ERPX_TENANT_DATABASE_URL_TEMPLATE",
];
const APPROVED_PROTO_DESCRIPTOR_REFERENCE: &str = "ERPX_G13_APPROVED_PROTO_DESCRIPTOR";
const CUTOVER_APPROVAL_REFERENCE: &str = "ERPX_G13_CUTOVER_APPROVAL_REFERENCE";

fn require_markers(surface: &str, name: &str, markers: &[&str]) {
    for marker in markers {
        assert!(
            surface.contains(marker),
            "G13 parallel acceptance marker missing from {name}: {marker}"
        );
    }
}

fn require_ordered(surface: &str, name: &str, markers: &[&str]) {
    let mut cursor = 0;
    for marker in markers {
        let offset = surface[cursor..].find(marker).unwrap_or_else(|| {
            panic!("G13 ordered marker missing from {name}: {marker}");
        });
        cursor += offset + marker.len();
    }
}

fn env_reference_is_nonempty(name: &str) -> bool {
    env::var(name)
        .ok()
        .as_deref()
        .is_some_and(reference_value_is_safe)
}

fn reference_value_is_safe(value: &str) -> bool {
    !value.trim().is_empty() && !value.chars().any(char::is_control)
}

fn canonical_postgres_authority(authority: &str) -> Option<String> {
    if authority.starts_with('[') {
        let closing_bracket = authority.find(']')?;
        let host = &authority[1..closing_bracket];
        if host.is_empty() {
            return None;
        }
        let suffix = &authority[closing_bracket + 1..];
        let port = match suffix {
            "" => 5432,
            suffix => suffix
                .strip_prefix(':')
                .and_then(|port| port.parse::<u16>().ok())
                .filter(|port| *port != 0)?,
        };
        return Some(format!("[{}]:{port}", host.to_ascii_lowercase()));
    }

    if authority.matches(':').count() > 1 {
        return None;
    }
    let (host, port) = authority
        .rsplit_once(':')
        .map_or((authority, 5432), |(host, port)| {
            (host, port.parse::<u16>().unwrap_or_default())
        });
    if host.is_empty() || (authority.contains(':') && port == 0) {
        return None;
    }
    Some(format!("{}:{port}", host.to_ascii_lowercase()))
}

/// Return the logical PostgreSQL database identity for an explicitly URL-shaped
/// runtime reference. Credentials and connection options are deliberately
/// excluded: changing either must not make a source/target/rollback collision
/// look isolated. Ambiguous forms return `None` so the caller fails closed.
fn database_reference_identity(value: &str) -> Option<String> {
    let value = value.trim();
    if !reference_value_is_safe(value) || value.contains('#') {
        return None;
    }

    let base = value.split_once('?').map_or(value, |(base, _)| base);
    let (scheme, authority_and_database) = base.split_once("://")?;
    let scheme = scheme.to_ascii_lowercase();
    if !matches!(scheme.as_str(), "postgres" | "postgresql") {
        return None;
    }

    let slash = authority_and_database.find('/')?;
    let authority = &authority_and_database[..slash];
    let database = &authority_and_database[slash + 1..];
    if authority.is_empty()
        || database.is_empty()
        || database.contains('/')
        || authority.chars().any(char::is_whitespace)
    {
        return None;
    }

    // Userinfo is not part of database isolation. Use the last `@` so a
    // percent-encoded or otherwise unusual username cannot create a second
    // identity for the same host/database pair.
    let host = authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host);
    let host = canonical_postgres_authority(host)?;

    Some(format!("postgresql://{host}/{database}"))
}

fn database_references_are_isolated(references: &[(&str, &str, &str)]) -> Result<(), String> {
    let mut identities = BTreeSet::new();
    for (label, name, value) in references {
        let identity = database_reference_identity(value).ok_or_else(|| {
            format!(
                "G13 {label} reference {name} is not an unambiguous PostgreSQL URL; isolation cannot be certified"
            )
        })?;
        if !identities.insert(identity.clone()) {
            return Err(format!(
                "G13 {label} reference {name} collides with another source/target/rollback database: {identity}"
            ));
        }
    }
    Ok(())
}

fn assert_configured_database_references_are_isolated() {
    let configured = [
        ("source", G13_DATABASE_REFERENCES[0]),
        ("target", G13_DATABASE_REFERENCES[1]),
        ("rollback", G13_DATABASE_REFERENCES[2]),
    ]
    .into_iter()
    .map(|(label, name)| {
        let value = env::var(name)
            .unwrap_or_else(|_| panic!("configured G13 {label} reference disappeared: {name}"));
        (label, name, value)
    })
    .collect::<Vec<_>>();
    let references = configured
        .iter()
        .map(|(label, name, value)| (*label, *name, value.as_str()))
        .collect::<Vec<_>>();
    database_references_are_isolated(&references).unwrap_or_else(|error| panic!("{error}"));
}

fn approved_descriptor_path() -> Option<PathBuf> {
    if let Some(path) = env::var_os(APPROVED_PROTO_DESCRIPTOR_REFERENCE)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
    {
        return Some(path);
    }
    let proto_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../crates/proto");
    [
        proto_root.join("compatibility/approved_descriptor.pb"),
        proto_root.join("compatibility/approved_descriptor.bin"),
        proto_root.join("compatibility/descriptor-set.pb"),
    ]
    .iter()
    .find(|path| path.is_file())
    .cloned()
}

fn approved_descriptor_reference_is_present() -> bool {
    approved_descriptor_path().is_some()
}

#[test]
fn g13_runtime_reference_gate_rejects_blank_and_controlled_values() {
    assert!(reference_value_is_safe(
        "postgresql://runtime.example.test/erpx?sslmode=verify-full"
    ));
    for value in ["", "   ", "\t", "postgresql://runtime.example.test/erpx\n"] {
        assert!(
            !reference_value_is_safe(value),
            "unsafe runtime reference must remain queued: {value:?}"
        );
    }
}

#[test]
fn g13_database_reference_identity_ignores_credentials_and_connection_options() {
    let source = database_reference_identity(
        "postgresql://source-user:secret@DB.EXAMPLE.test/erpx_source?sslmode=verify-full&connect_timeout=5",
    );
    let equivalent = database_reference_identity(
        "postgres://other-user:other-secret@db.example.TEST:5432/erpx_source?sslmode=require",
    );
    assert_eq!(source, equivalent);
    assert!(
        database_reference_identity("postgresql://db.example.test/erpx_source#fragment").is_none()
    );
    assert!(database_reference_identity("mysql://db.example.test/erpx_source").is_none());
    assert!(database_reference_identity("postgresql://db.example.test/").is_none());
    assert_eq!(
        database_reference_identity("postgresql://[2001:DB8::1]:5432/erpx_source"),
        Some("postgresql://[2001:db8::1]:5432/erpx_source".to_owned())
    );
    for malformed in [
        "postgresql://[::1]trailing/erpx_source",
        "postgresql://[::1]:0/erpx_source",
        "postgresql://[::1]:5432trailing/erpx_source",
    ] {
        assert!(
            database_reference_identity(malformed).is_none(),
            "malformed bracketed authority must fail closed: {malformed}"
        );
    }
}

#[test]
fn g13_database_reference_isolation_rejects_logical_collisions() {
    let result = database_references_are_isolated(&[
        (
            "source",
            "ERPX_G13_SOURCE_DATABASE_URL",
            "postgresql://source-user:secret@db.example.test/erpx_source?sslmode=verify-full",
        ),
        (
            "target",
            "ERPX_G13_TARGET_DATABASE_URL",
            "postgres://target-user:other-secret@DB.EXAMPLE.TEST/erpx_source?sslmode=require",
        ),
        (
            "rollback",
            "ERPX_G13_ROLLBACK_DATABASE_URL",
            "postgresql://db.example.test/erpx_rollback?sslmode=verify-full",
        ),
    ]);
    let error = result.expect_err("source/target logical database collision must fail closed");
    assert!(error.contains("target"));
    assert!(error.contains("erpx_source"));
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FieldContract {
    name: String,
    number: i32,
    label: i32,
    field_type: i32,
    type_name: String,
    json_name: String,
    oneof_index: Option<i32>,
    proto3_optional: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct MethodContract {
    input_type: String,
    output_type: String,
    client_streaming: bool,
    server_streaming: bool,
}

#[derive(Debug, Default)]
struct DescriptorInventory {
    messages: BTreeMap<String, MessageContract>,
    enums: BTreeMap<String, EnumContract>,
    services: BTreeMap<String, BTreeMap<String, MethodContract>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct MessageContract {
    fields: BTreeMap<i32, FieldContract>,
    reserved_ranges: BTreeSet<(i32, i32)>,
    reserved_names: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct EnumContract {
    values: BTreeMap<i32, String>,
    reserved_ranges: BTreeSet<(i32, i32)>,
    reserved_names: BTreeSet<String>,
}

fn qualified_name(prefix: &str, name: &str) -> String {
    if prefix.is_empty() {
        name.to_owned()
    } else {
        format!("{prefix}.{name}")
    }
}

fn add_message_contract(
    message: &prost_types::DescriptorProto,
    prefix: &str,
    inventory: &mut DescriptorInventory,
) {
    let Some(name) = message.name.as_deref() else {
        return;
    };
    let full_name = qualified_name(prefix, name);
    let fields = message
        .field
        .iter()
        .filter_map(|field| {
            let number = field.number?;
            let field_name = field.name.clone()?;
            Some((
                number,
                FieldContract {
                    name: field_name,
                    number,
                    label: field.label.unwrap_or_default(),
                    field_type: field.r#type.unwrap_or_default(),
                    type_name: field.type_name.clone().unwrap_or_default(),
                    json_name: field.json_name.clone().unwrap_or_default(),
                    oneof_index: field.oneof_index,
                    proto3_optional: field.proto3_optional.unwrap_or(false),
                },
            ))
        })
        .collect();
    let reserved_ranges = message
        .reserved_range
        .iter()
        .filter_map(|range| Some((range.start?, range.end?)))
        .collect();
    let reserved_names = message.reserved_name.iter().cloned().collect();
    inventory.messages.insert(
        full_name.clone(),
        MessageContract {
            fields,
            reserved_ranges,
            reserved_names,
        },
    );
    for nested in &message.nested_type {
        add_message_contract(nested, &full_name, inventory);
    }
    for enum_type in &message.enum_type {
        add_enum_contract(enum_type, &full_name, inventory);
    }
}

fn add_enum_contract(
    enum_type: &prost_types::EnumDescriptorProto,
    prefix: &str,
    inventory: &mut DescriptorInventory,
) {
    let Some(name) = enum_type.name.as_deref() else {
        return;
    };
    let full_name = qualified_name(prefix, name);
    let values = enum_type
        .value
        .iter()
        .filter_map(|value| Some((value.number?, value.name.clone()?)))
        .collect();
    let reserved_ranges = enum_type
        .reserved_range
        .iter()
        .filter_map(|range| Some((range.start?, range.end?)))
        .collect();
    let reserved_names = enum_type.reserved_name.iter().cloned().collect();
    inventory.enums.insert(
        full_name,
        EnumContract {
            values,
            reserved_ranges,
            reserved_names,
        },
    );
}

fn descriptor_inventory(set: &FileDescriptorSet) -> DescriptorInventory {
    let mut inventory = DescriptorInventory::default();
    for file in &set.file {
        let package = file.package.as_deref().unwrap_or_default();
        for message in &file.message_type {
            add_message_contract(message, package, &mut inventory);
        }
        for enum_type in &file.enum_type {
            add_enum_contract(enum_type, package, &mut inventory);
        }
        for service in &file.service {
            let Some(name) = service.name.as_deref() else {
                continue;
            };
            let full_name = qualified_name(package, name);
            let methods = service
                .method
                .iter()
                .filter_map(|method| {
                    let method_name = method.name.clone()?;
                    Some((
                        method_name,
                        MethodContract {
                            input_type: method.input_type.clone().unwrap_or_default(),
                            output_type: method.output_type.clone().unwrap_or_default(),
                            client_streaming: method.client_streaming.unwrap_or(false),
                            server_streaming: method.server_streaming.unwrap_or(false),
                        },
                    ))
                })
                .collect();
            inventory.services.insert(full_name, methods);
        }
    }
    inventory
}

fn validate_descriptor_set_for_comparison(
    set: &FileDescriptorSet,
    label: &str,
) -> Result<(), String> {
    if set.file.is_empty() {
        return Err(format!("{label} protobuf descriptor is empty"));
    }
    let mut names = BTreeSet::new();
    let mut declarations = BTreeSet::new();
    let mut declaration_count = 0_usize;
    for file in &set.file {
        let name = file
            .name
            .as_deref()
            .filter(|name| !name.is_empty())
            .ok_or_else(|| format!("{label} protobuf descriptor has a missing file name"))?;
        if !names.insert(name) {
            return Err(format!(
                "{label} protobuf descriptor has a duplicate file: {name}"
            ));
        }
        let package = file.package.as_deref().unwrap_or_default();
        for message in &file.message_type {
            validate_message_declaration(message, package, label, &mut declarations)?;
            declaration_count += 1;
        }
        for enum_type in &file.enum_type {
            validate_enum_declaration(enum_type, package, label, &mut declarations)?;
            declaration_count += 1;
        }
        for service in &file.service {
            let service_name = service
                .name
                .as_deref()
                .filter(|name| !name.is_empty())
                .ok_or_else(|| format!("{label} protobuf descriptor has a missing service name"))?;
            let full_name = qualified_name(package, service_name);
            if !declarations.insert(full_name.clone()) {
                return Err(format!(
                    "{label} protobuf descriptor has a duplicate declaration: {full_name}"
                ));
            }
            let mut methods = BTreeSet::new();
            for method in &service.method {
                let method_name = method
                    .name
                    .as_deref()
                    .filter(|name| !name.is_empty())
                    .ok_or_else(|| {
                        format!("{label} protobuf descriptor has a missing RPC method name")
                    })?;
                if !methods.insert(method_name) {
                    return Err(format!(
                        "{label} protobuf descriptor has a duplicate RPC method: {full_name}.{method_name}"
                    ));
                }
                if method.input_type.as_deref().is_none_or(str::is_empty)
                    || method.output_type.as_deref().is_none_or(str::is_empty)
                {
                    return Err(format!(
                        "{label} protobuf descriptor has an incomplete RPC method: {full_name}.{method_name}"
                    ));
                }
            }
            declaration_count += 1;
        }
    }
    if declaration_count == 0 {
        return Err(format!(
            "{label} protobuf descriptor contains no declarations"
        ));
    }
    Ok(())
}

fn validate_message_declaration(
    message: &prost_types::DescriptorProto,
    prefix: &str,
    label: &str,
    declarations: &mut BTreeSet<String>,
) -> Result<(), String> {
    let name = message
        .name
        .as_deref()
        .filter(|name| !name.is_empty())
        .ok_or_else(|| format!("{label} protobuf descriptor has a missing message name"))?;
    let full_name = qualified_name(prefix, name);
    if !declarations.insert(full_name.clone()) {
        return Err(format!(
            "{label} protobuf descriptor has a duplicate declaration: {full_name}"
        ));
    }

    let mut field_numbers = BTreeSet::new();
    let mut field_names = BTreeSet::new();
    for field in &message.field {
        let field_name = field
            .name
            .as_deref()
            .filter(|name| !name.is_empty())
            .ok_or_else(|| {
                format!("{label} protobuf descriptor has a missing field name: {full_name}")
            })?;
        let number = field.number.ok_or_else(|| {
            format!(
                "{label} protobuf descriptor has a missing field number: {full_name}.{field_name}"
            )
        })?;
        if !field_numbers.insert(number) {
            return Err(format!(
                "{label} protobuf descriptor has a duplicate field number: {full_name}.{number}"
            ));
        }
        if !field_names.insert(field_name) {
            return Err(format!(
                "{label} protobuf descriptor has a duplicate field name: {full_name}.{field_name}"
            ));
        }
        if !(1..=536_870_911).contains(&number) {
            return Err(format!(
                "{label} protobuf descriptor has an invalid field number: {full_name}.{number}"
            ));
        }
    }
    for reserved in &message.reserved_range {
        let start = reserved.start.ok_or_else(|| {
            format!("{label} protobuf descriptor has an incomplete reserved range: {full_name}")
        })?;
        let end = reserved.end.ok_or_else(|| {
            format!("{label} protobuf descriptor has an incomplete reserved range: {full_name}")
        })?;
        if start >= end {
            return Err(format!(
                "{label} protobuf descriptor has an invalid reserved range: {full_name}"
            ));
        }
        if field_numbers
            .iter()
            .any(|number| start <= *number && *number < end)
        {
            return Err(format!(
                "{label} protobuf descriptor reuses a reserved field number: {full_name}"
            ));
        }
    }

    for nested in &message.nested_type {
        validate_message_declaration(nested, &full_name, label, declarations)?;
    }
    for enum_type in &message.enum_type {
        validate_enum_declaration(enum_type, &full_name, label, declarations)?;
    }
    Ok(())
}

fn validate_enum_declaration(
    enum_type: &prost_types::EnumDescriptorProto,
    prefix: &str,
    label: &str,
    declarations: &mut BTreeSet<String>,
) -> Result<(), String> {
    let name = enum_type
        .name
        .as_deref()
        .filter(|name| !name.is_empty())
        .ok_or_else(|| format!("{label} protobuf descriptor has a missing enum name"))?;
    let full_name = qualified_name(prefix, name);
    if !declarations.insert(full_name.clone()) {
        return Err(format!(
            "{label} protobuf descriptor has a duplicate declaration: {full_name}"
        ));
    }

    let mut value_numbers = BTreeSet::new();
    let mut value_names = BTreeSet::new();
    for value in &enum_type.value {
        let value_name = value
            .name
            .as_deref()
            .filter(|name| !name.is_empty())
            .ok_or_else(|| {
                format!("{label} protobuf descriptor has a missing enum value name: {full_name}")
            })?;
        let number = value.number.ok_or_else(|| {
            format!("{label} protobuf descriptor has a missing enum value number: {full_name}.{value_name}")
        })?;
        if !value_numbers.insert(number) {
            return Err(format!(
                "{label} protobuf descriptor has a duplicate enum value number: {full_name}.{number}"
            ));
        }
        if !value_names.insert(value_name) {
            return Err(format!(
                "{label} protobuf descriptor has a duplicate enum value name: {full_name}.{value_name}"
            ));
        }
    }
    for reserved in &enum_type.reserved_range {
        let start = reserved.start.ok_or_else(|| {
            format!(
                "{label} protobuf descriptor has an incomplete enum reserved range: {full_name}"
            )
        })?;
        let end = reserved.end.ok_or_else(|| {
            format!(
                "{label} protobuf descriptor has an incomplete enum reserved range: {full_name}"
            )
        })?;
        if start >= end {
            return Err(format!(
                "{label} protobuf descriptor has an invalid enum reserved range: {full_name}"
            ));
        }
        if value_numbers
            .iter()
            .any(|number| start <= *number && *number < end)
        {
            return Err(format!(
                "{label} protobuf descriptor reuses a reserved enum value: {full_name}"
            ));
        }
    }
    Ok(())
}

fn compare_descriptor_compatibility(
    approved_bytes: &[u8],
    current_bytes: &[u8],
) -> Result<(), String> {
    let approved = FileDescriptorSet::decode(approved_bytes)
        .map_err(|error| format!("approved protobuf descriptor is invalid: {error}"))?;
    let current = FileDescriptorSet::decode(current_bytes)
        .map_err(|error| format!("current protobuf descriptor is invalid: {error}"))?;
    validate_descriptor_set_for_comparison(&approved, "approved")?;
    validate_descriptor_set_for_comparison(&current, "current")?;
    let approved = descriptor_inventory(&approved);
    let current = descriptor_inventory(&current);

    for (message_name, expected) in approved.messages {
        let actual = current
            .messages
            .get(&message_name)
            .ok_or_else(|| format!("approved message was removed: {message_name}"))?;
        for (number, expected_field) in expected.fields {
            let actual_field = actual
                .fields
                .get(&number)
                .ok_or_else(|| format!("approved field was removed: {message_name}.{number}"))?;
            if actual_field != &expected_field {
                return Err(format!(
                    "approved field changed incompatibly: {message_name}.{number}"
                ));
            }
        }
        if !expected.reserved_ranges.is_subset(&actual.reserved_ranges) {
            return Err(format!(
                "approved message reserved range was removed: {message_name}"
            ));
        }
        if !expected.reserved_names.is_subset(&actual.reserved_names) {
            return Err(format!(
                "approved message reserved name was removed: {message_name}"
            ));
        }
    }
    for (enum_name, expected) in approved.enums {
        let actual = current
            .enums
            .get(&enum_name)
            .ok_or_else(|| format!("approved enum was removed: {enum_name}"))?;
        for (number, expected_name) in expected.values {
            let actual_name = actual
                .values
                .get(&number)
                .ok_or_else(|| format!("approved enum value was removed: {enum_name}.{number}"))?;
            if actual_name != &expected_name {
                return Err(format!(
                    "approved enum value changed incompatibly: {enum_name}.{number}"
                ));
            }
        }
        if !expected.reserved_ranges.is_subset(&actual.reserved_ranges) {
            return Err(format!(
                "approved enum reserved range was removed: {enum_name}"
            ));
        }
        if !expected.reserved_names.is_subset(&actual.reserved_names) {
            return Err(format!(
                "approved enum reserved name was removed: {enum_name}"
            ));
        }
    }
    for (service_name, methods) in approved.services {
        let current_methods = current
            .services
            .get(&service_name)
            .ok_or_else(|| format!("approved service was removed: {service_name}"))?;
        for (method_name, expected) in methods {
            let actual = current_methods.get(&method_name).ok_or_else(|| {
                format!("approved RPC method was removed: {service_name}.{method_name}")
            })?;
            if actual != &expected {
                return Err(format!(
                    "approved RPC method changed incompatibly: {service_name}.{method_name}"
                ));
            }
        }
    }
    Ok(())
}

#[test]
fn g13_deployment_phases_and_migrator_boundary_are_ordered_and_read_only() {
    require_markers(
        MIGRATION_LIB,
        "crates/migration/src/lib.rs",
        &[
            "CONTROL_PLANE_MIGRATOR",
            "CONTROL_READ_MIGRATOR",
            "TENANT_WRITE_MIGRATOR",
            "TENANT_READ_MIGRATOR",
            "pub enum MigrationPhase",
            "pub enum CommandOwner",
            "SET TRANSACTION READ ONLY",
            "read-only transaction rollback",
        ],
    );
    require_ordered(
        MIGRATION_LIB,
        "crates/migration/src/lib.rs",
        &[
            "SourceFrozen",
            "RollbackRehearsed",
            "FinalDeltaImported",
            "ErpxCommandOwner",
            "FolioReadOnly",
            "RetentionComplete",
            "FolioRetired",
        ],
    );
    require_markers(
        MIGRATOR_MAIN,
        "apps/migrator/src/main.rs",
        &[
            "read-only and dry-run by default",
            "rollback_rehearsal",
            "final_delta",
            "command_owner_switch",
            "folio_read_only_retention",
            "performed: false",
            "state_change: false",
            "read_only: true",
        ],
    );
    assert!(
        !MIGRATOR_MAIN.contains("Command::Cutover")
            && !MIGRATOR_MAIN.contains("perform_cutover")
            && !MIGRATOR_MAIN.contains("switch_command_owner"),
        "the read-only migrator must not own a production cutover command"
    );
}

#[test]
fn g13_authoritative_migrations_and_protobuf_wire_policy_are_structurally_bound() {
    require_markers(
        CONTROL_MIGRATION,
        "migrations/0001_control_plane.sql",
        &[
            "erpx_control.idempotency_keys",
            "erpx_control.outbox_events",
            "erpx_control.projection_checkpoints",
            "CONSTRAINT outbox_idempotency_key_unique UNIQUE (idempotency_key)",
        ],
    );
    require_markers(
        COMMAND_RECEIPTS_MIGRATION,
        "migrations/0002_g1_002_command_receipts.sql",
        &[
            "ALTER TABLE erpx_control.idempotency_keys",
            "CREATE TABLE IF NOT EXISTS erpx_control.projection_event_receipts",
            "PRIMARY KEY (projection_name, event_id)",
        ],
    );
    require_markers(
        PROTO_BUILD,
        "crates/proto/build.rs",
        &[
            "rerun-if-changed=proto/erpx/common/v1/context.proto",
            "rerun-if-changed=proto/erpx/platform/v1/platform.proto",
            "build_client(true)",
            "build_server(true)",
            "compile_protos(",
            "\"proto/erpx/common/v1/context.proto\"",
            "\"proto/erpx/platform/v1/platform.proto\"",
        ],
    );
    require_markers(
        PROTO_LIB,
        "crates/proto/src/lib.rs",
        &[
            "tonic::include_proto!(\"erpx.common.v1\")",
            "tonic::include_proto!(\"erpx.platform.v1\")",
        ],
    );
    require_markers(
        COMMON_PROTO,
        "common/v1/context.proto",
        &[
            "package erpx.common.v1;",
            "message CommandContext",
            "message QueryContext",
        ],
    );
    require_markers(
        PLATFORM_PROTO,
        "platform/v1/platform.proto",
        &[
            "package erpx.platform.v1;",
            "service PlatformOperationsService",
            "service DirectoryService",
            "service SalesService",
            "service ScmService",
            "service AssetMaintenanceService",
            "service LegalService",
            "reserved",
            "wire compatibility",
        ],
    );
    require_markers(
        CONTRACT_POLICY,
        "CONTRACT_POLICY.md",
        &[
            "Protobuf packages use a versioned namespace",
            "Existing field numbers and meanings are never reused",
            "Commands must carry an idempotency key",
            "Every contract change adds a compatibility test",
        ],
    );
}

#[test]
fn g13_latest_local_compatibility_and_registration_surfaces_are_bound() {
    require_markers(
        G13_CONTROL_MIGRATION,
        "migrations/0058_migration_execution.sql",
        &[
            "migration_runs",
            "migration_transition_events",
            "migration_idempotency",
            "migration_checkpoints",
            "migration_reconciliation_reports",
            "migration_rollback_rehearsals",
            "command ownership",
        ],
    );
    require_markers(
        G13_READ_MIGRATION,
        "migrations_read/0011_migration_status.sql",
        &[
            "migration_status_read_model",
            "migration_reconciliation_read_model",
            "migration_projection_checkpoints",
        ],
    );
    require_markers(
        G13_SOURCE_SNAPSHOT_MIGRATION,
        "migrations/0059_migration_source_snapshots.sql",
        &[
            "migration_source_snapshots",
            "source_contract_hash",
            "octet_length(snapshot::text) <= 16777216",
            "UNIQUE (run_id, source_contract_hash)",
        ],
    );
    require_markers(
        G13_SOURCE_SNAPSHOT_READ_MIGRATION,
        "migrations_read/0012_migration_source_snapshot_status.sql",
        &[
            "source_snapshot_hash",
            "source_missing_table_count",
            "source_entity_kind_count",
            "source_total_rows",
        ],
    );
    require_markers(
        G13_SERVER,
        "apps/server/src/migration.rs",
        &[
            "list_migration_statuses",
            "MIGRATION_STATUS_READ_PERMISSION",
            "queue_migration_work",
            "begin_migration_run",
            "advance_migration_phase",
            "authorize_mfa",
            "QueueMigrationWorkRequest",
            "BeginMigrationRunRequest",
            "AdvanceMigrationPhaseRequest",
            "cutover mutation",
        ],
    );
    require_markers(
        G13_WORKER_RUNTIME,
        "apps/worker/src/g13_migration_runtime.rs",
        &[
            "G13_MIGRATION_HANDLER_NAME",
            "ERPX_G13_WORKER_MFA_VERIFIED",
            "project_read_model",
            "never changes command ownership",
        ],
    );
    require_markers(
        SERVER_MAIN,
        "apps/server/src/main.rs",
        &[
            "DirectoryServiceServer::new",
            "PlatformOperationsServiceServer::new",
            "SalesServiceServer::new",
            "ScmServiceServer::new",
            "AssetMaintenanceServiceServer::new",
            "LegalServiceServer::new",
            "MigrationServiceServer::new",
        ],
    );
    require_markers(
        WORKER_MAIN,
        "apps/worker/src/main.rs",
        &[
            "DIRECTORY_SYNC_HANDLER_NAME",
            "OPERATIONS_COMPLETION_HANDLER_NAME",
            "HR_COMPLETION_HANDLER_NAME",
            "FINANCE_COMPLETION_HANDLER_NAME",
            "ERPX_TENANT_PROJECTION_KIND",
            "asset_maintenance",
            "legal_documents",
            "scm",
            "pos",
            "hr",
            "sales",
            "marketing",
            "G13_MIGRATION_HANDLER_NAME",
        ],
    );
    require_markers(
        CONSOLE_MAIN,
        "apps/console/src/main.rs",
        &[
            "DirectoryServiceClient",
            "DirectoryAdministrationRouteSurface",
            "DirectoryAdministrationSurface",
            "ListDirectoryConnectorsRequest",
            "ListDirectoryRoleMappingsRequest",
            "/platform/directories",
            "MigrationServiceClient",
            "MigrationStatusList",
        ],
    );
    require_markers(
        CONSOLE_PRODUCT_COMPLETION,
        "apps/console/src/product_completion.rs",
        &[
            "DIRECTORY_READ_MODEL_AUTHORITY",
            "DirectoryAdministrationSurface",
            "data-read-only",
            "Append-only",
            "Immutable",
            "/platform/directories",
        ],
    );
    eprintln!(
        "G13 local compatibility/registration evidence: versioned platform protobuf, authenticated server services, worker handlers/projection kinds, and the bounded directory/migration console routes are present"
    );
}

#[test]
fn g13_approved_descriptor_compatibility_is_executed_when_baseline_exists() {
    let Some(path) = approved_descriptor_path() else {
        eprintln!(
            "G13 queued protobuf acceptance: no approved descriptor reference is configured; compatibility comparison remains queued"
        );
        return;
    };
    let approved = fs::read(&path).unwrap_or_else(|error| {
        panic!(
            "G13 approved protobuf descriptor could not be read from {}: {error}",
            path.display()
        )
    });
    compare_descriptor_compatibility(&approved, erpx_proto::FILE_DESCRIPTOR_SET)
        .unwrap_or_else(|error| panic!("G13 protobuf compatibility comparison failed: {error}"));
    eprintln!("G13 protobuf compatibility comparison passed against the approved descriptor");
}

#[test]
fn g13_descriptor_comparison_rejects_empty_and_duplicate_file_baselines() {
    let mut empty = Vec::new();
    FileDescriptorSet { file: Vec::new() }
        .encode(&mut empty)
        .expect("encode empty descriptor");
    let error = compare_descriptor_compatibility(&empty, erpx_proto::FILE_DESCRIPTOR_SET)
        .expect_err("empty approved descriptor must not certify compatibility");
    assert!(error.contains("empty"));

    let mut duplicate = Vec::new();
    FileDescriptorSet {
        file: vec![
            FileDescriptorProto {
                name: Some("duplicate.proto".into()),
                ..FileDescriptorProto::default()
            },
            FileDescriptorProto {
                name: Some("duplicate.proto".into()),
                ..FileDescriptorProto::default()
            },
        ],
    }
    .encode(&mut duplicate)
    .expect("encode duplicate descriptor");
    let error = compare_descriptor_compatibility(&duplicate, erpx_proto::FILE_DESCRIPTOR_SET)
        .expect_err("duplicate descriptor files must not certify compatibility");
    assert!(error.contains("duplicate file"));
}

#[test]
fn g13_descriptor_comparison_rejects_empty_and_malformed_declarations() {
    let mut empty = Vec::new();
    FileDescriptorSet {
        file: vec![FileDescriptorProto {
            name: Some("empty.proto".into()),
            ..FileDescriptorProto::default()
        }],
    }
    .encode(&mut empty)
    .expect("encode empty declaration descriptor");
    let error = compare_descriptor_compatibility(&empty, erpx_proto::FILE_DESCRIPTOR_SET)
        .expect_err("a named but declaration-free descriptor must not certify compatibility");
    assert!(error.contains("no declarations"));

    let mut malformed = FileDescriptorSet::decode(erpx_proto::FILE_DESCRIPTOR_SET)
        .expect("current descriptor must decode");
    let message = malformed
        .file
        .iter_mut()
        .find_map(|file| file.message_type.first_mut())
        .expect("current descriptor must contain a message");
    let first = message
        .field
        .first()
        .cloned()
        .expect("current descriptor must contain a field");
    message.field.push(prost_types::FieldDescriptorProto {
        name: Some("duplicate_field".into()),
        number: first.number,
        ..prost_types::FieldDescriptorProto::default()
    });
    let mut malformed_bytes = Vec::new();
    malformed
        .encode(&mut malformed_bytes)
        .expect("encode malformed descriptor");
    let error = compare_descriptor_compatibility(&malformed_bytes, erpx_proto::FILE_DESCRIPTOR_SET)
        .expect_err("duplicate field numbers must not certify compatibility");
    assert!(error.contains("duplicate field number"));
}

#[test]
fn g13_descriptor_comparison_rejects_removed_reserved_contracts() {
    let mut approved = FileDescriptorSet::decode(erpx_proto::FILE_DESCRIPTOR_SET)
        .expect("current descriptor must decode");
    let message = approved
        .file
        .iter_mut()
        .find_map(|file| file.message_type.first_mut())
        .expect("current descriptor must contain a message");
    message
        .reserved_range
        .push(prost_types::descriptor_proto::ReservedRange {
            start: Some(1900),
            end: Some(1901),
        });
    message.reserved_name.push("erpx_legacy_field".into());

    let mut approved_bytes = Vec::new();
    approved
        .encode(&mut approved_bytes)
        .expect("encode reserved message baseline");
    let error = compare_descriptor_compatibility(&approved_bytes, erpx_proto::FILE_DESCRIPTOR_SET)
        .expect_err("removed message reservations must fail compatibility");
    assert!(error.contains("reserved range") || error.contains("reserved name"));

    let mut approved = FileDescriptorSet::decode(erpx_proto::FILE_DESCRIPTOR_SET)
        .expect("current descriptor must decode");
    let enum_type = approved
        .file
        .iter_mut()
        .find_map(|file| file.enum_type.first_mut())
        .expect("current descriptor must contain an enum");
    enum_type
        .reserved_range
        .push(prost_types::enum_descriptor_proto::EnumReservedRange {
            start: Some(1900),
            end: Some(1901),
        });
    enum_type.reserved_name.push("ERPX_LEGACY_VALUE".into());

    let mut approved_bytes = Vec::new();
    approved
        .encode(&mut approved_bytes)
        .expect("encode reserved enum baseline");
    let error = compare_descriptor_compatibility(&approved_bytes, erpx_proto::FILE_DESCRIPTOR_SET)
        .expect_err("removed enum reservations must fail compatibility");
    assert!(error.contains("reserved range") || error.contains("reserved name"));
}

#[test]
fn g13_folio_bridge_is_read_only_tls_bound_and_queues_real_work() {
    require_markers(
        G13_FOLIO_BRIDGE,
        "apps/worker/src/folio_bridge.rs",
        &[
            "FolioSourceExtractor",
            "ERPX_G13_SOURCE_DATABASE_URL",
            "sslmode=verify-full",
            "record_source_snapshot",
            "enqueue_work",
            "G13_FOLIO_BRIDGE_HANDLER_NAME",
            "does not write Folio",
        ],
    );
    assert!(
        G13_FOLIO_BRIDGE.contains("for kind in snapshot.id_mappings.keys()"),
        "G13 Folio bridge must queue every identity-bearing source scope"
    );
    assert!(
        !G13_FOLIO_BRIDGE.contains("id_mappings.keys().take(64)"),
        "G13 Folio bridge must not silently truncate source scopes"
    );
    require_markers(
        WORKER_MAIN,
        "apps/worker/src/main.rs",
        &[
            "handler_name == G13_FOLIO_BRIDGE_HANDLER_NAME",
            "FolioBridgeRuntimeHandler::from_env",
        ],
    );
}

#[test]
fn g13_restart_recovery_and_idempotency_seams_are_authoritative() {
    require_markers(
        WORKER_LIB,
        "apps/worker/src/lib.rs",
        &[
            "pub struct WorkerHandlerReport",
            "pub trait NamedWorkerHandler",
            "pub struct WorkerHandlerRegistry",
            "pub async fn run_registered_until_stopped",
            "pub async fn run_registered_until_shutdown",
            "run_registered_handler_until_stopped",
            "record_worker_heartbeat",
            "retry_policy",
        ],
    );
    require_markers(
        WORKER_MAIN,
        "apps/worker/src/main.rs",
        &[
            "DIRECTORY_SYNC_HANDLER_NAME",
            "OPERATIONS_COMPLETION_HANDLER_NAME",
            "run_registered_until_shutdown",
            "retried = report.retried",
            "dead_lettered = report.dead_lettered",
        ],
    );
    require_markers(
        PROJECTION_SCHEDULER,
        "crates/infrastructure/src/tenant_projection_scheduler.rs",
        &[
            "claim_key",
            "lease_until",
            "retry budget is exhausted",
            "dead_letter",
            "ON CONFLICT (claim_key) DO NOTHING",
            "claim_key_retries_are_idempotent_and_scope_bound",
        ],
    );
    require_markers(
        SERVER_MAIN,
        "apps/server/src/main.rs",
        &[
            "\"/healthz\"",
            "\"/readyz\"",
            "\"/metrics\"",
            "HttpRequestTrace::start",
            "worker_heartbeat",
            "projection_freshness",
        ],
    );
}

#[test]
fn g13_disposable_acceptance_is_real_and_cutover_free() {
    require_markers(
        G13_DISPOSABLE_ACCEPTANCE,
        "crates/infrastructure/tests/g13_disposable_acceptance.rs",
        &[
            "CREATE DATABASE",
            "WITH (FORCE)",
            "migrate_control_plane",
            "shadow_replay",
            "reconcile_run",
            "rehearse_rollback",
            "expire_rehearsal",
            "source database was mutated by the G13 acceptance",
            "unexpectedly advanced command ownership",
            "#[ignore = \"creates six disposable local PostgreSQL databases; run explicitly with --ignored\"]",
        ],
    );
}

#[test]
fn g13_folio_bridge_remains_permission_gated_and_legacy_owned() {
    require_markers(
        FOLIO_PLATFORM_PAGE,
        "app/(app)/(protected)/platform/page.tsx",
        &[
            "export const dynamic = 'force-dynamic'",
            "canAccessPlatform(actor)",
            "loadPlatformOverview()",
            "schemaReady",
            "NoPermissionView",
        ],
    );
    require_markers(
        FOLIO_PLATFORM_ACCESS,
        "lib/platform/access.ts",
        &[
            "PLATFORM_VIEW",
            "canAccessPlatform",
            "platformApiGuard",
            "admin:system:bypass::allow",
        ],
    );
    require_markers(
        FOLIO_PLATFORM_QUERIES,
        "lib/platform/queries.ts",
        &[
            "controlReadQuery",
            "loadPlatformOverview",
            "recordGlobalRead",
        ],
    );
    require_markers(
        FOLIO_PLATFORM_CONSOLE,
        "components/platform/PlatformConsole.tsx",
        &[
            "Control plane is not initialized",
            "without changing tenant business records",
            "schemaReady",
        ],
    );
    require_markers(
        FOLIO_CONTROL_MIGRATION,
        "db/migrations/2026-07-23-erpx-control-plane.sql",
        &[
            "Tenant business data remains in Folio's",
            "CREATE SCHEMA IF NOT EXISTS platform",
            "CREATE TABLE IF NOT EXISTS platform.customer_accounts",
        ],
    );
}

#[test]
fn g13_runtime_acceptance_is_explicitly_queued_without_side_effects() {
    for (label, name) in [
        ("source", G13_DATABASE_REFERENCES[0]),
        ("target", G13_DATABASE_REFERENCES[1]),
        ("rollback", G13_DATABASE_REFERENCES[2]),
    ] {
        if env_reference_is_nonempty(name) {
            eprintln!(
                "G13 {label} database reference present: disposable runtime execution remains queued"
            );
        } else {
            eprintln!("G13 {label} database blocker queued: missing non-empty {name}");
        }
    }
    if G13_DATABASE_REFERENCES
        .iter()
        .copied()
        .all(env_reference_is_nonempty)
    {
        assert_configured_database_references_are_isolated();
        eprintln!(
            "G13 source/target/rollback database isolation guard passed for the configured disposable references"
        );
    } else {
        eprintln!(
            "G13 source/target/rollback isolation blocker queued: all three canonical disposable PostgreSQL references are required before isolation can be certified"
        );
    }

    let missing_runtime_references = G13_RUNTIME_DATABASE_REFERENCES
        .iter()
        .copied()
        .filter(|name| !env_reference_is_nonempty(name))
        .collect::<Vec<_>>();
    if missing_runtime_references.is_empty() {
        eprintln!(
            "G13 runtime database references present; migration, worker, rollback, and reconciliation execution remains queued"
        );
    } else {
        eprintln!(
            "G13 runtime blocker queued: missing control/tenant runtime references: {}",
            missing_runtime_references.join(", ")
        );
    }

    if approved_descriptor_reference_is_present() {
        eprintln!(
            "G13 protobuf compatibility baseline is present; the dedicated descriptor comparison test executes it"
        );
    } else {
        eprintln!(
            "G13 queued protobuf acceptance: no approved descriptor reference is configured; compatibility comparison remains unexecuted"
        );
    }

    eprintln!(
        "G13 runtime execution blocker queued: source extraction, target import, migration replay, two-worker lease-loss/restart, outbox/idempotency, rollback restore, and post-restore reconciliation evidence remains unexecuted"
    );
    eprintln!(
        "G13 cutover blocker queued: current command ownership, Folio write-retention, rollback, and post-cutover guard evidence remains unexecuted"
    );
    if env_reference_is_nonempty(CUTOVER_APPROVAL_REFERENCE) {
        eprintln!(
            "G13 cutover approval reference present, but this read-only packet will not switch command ownership or retire Folio writes"
        );
    } else {
        eprintln!(
            "G13 cutover blocker queued: missing explicit approval reference {CUTOVER_APPROVAL_REFERENCE}; no production cutover is authorized"
        );
    }
    eprintln!(
        "G13 parallel acceptance is read-only: no SQL, process, network, migration, connector, cutover, or Folio write operation was performed"
    );

    // Keep the queue test fail-closed if a future implementation adds a
    // production cutover command to this read-only migrator.
    assert!(!MIGRATOR_MAIN.contains("Command::Cutover"));
    assert!(!MIGRATOR_MAIN.contains("perform_cutover"));
    assert!(!MIGRATOR_MAIN.contains("switch_command_owner"));
}
