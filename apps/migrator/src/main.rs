use erpx_migration::{
    DeterministicIdMapper, ReconciliationSnapshot, load_deterministic_fixture, reconcile_snapshots,
};
use serde::de::{self, DeserializeSeed, Deserializer, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fmt;
use std::fs;
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};
use uuid::Uuid;

const MAX_JSON_BYTES: usize = 128 * 1024;
const MAX_MAP_ENTRIES: usize = 256;
const MAX_SNAPSHOT_ENTRIES: usize = 512;
const MAX_EXPLANATIONS: usize = 512;
const MAX_KEY_BYTES: usize = 128;
const MAX_EXPLANATION_BYTES: usize = 2 * 1024;

const HELP: &str = "ERPX migration tooling (read-only and dry-run by default)\n\n\
USAGE:\n  erpx-migrator <COMMAND> [OPTIONS]\n\n\
COMMANDS:\n  fixture     Emit the validated anonymized fixture summary\n  map         Map a bounded JSON object of source IDs\n  reconcile   Compare two bounded reconciliation snapshots\n  plan        Emit the read-only migration execution plan\n\n\
COMMON OPTIONS:\n  --output PATH  Write JSON to a safe file path (stdout is the default)\n  -h, --help     Show this help\n\n\
MAP OPTIONS:\n  --namespace NAME  Required deterministic mapping namespace\n  --context NAME    Optional context for every source ID (object keys are used otherwise)\n  --input PATH      Required JSON object whose values are UUID source IDs\n\n\
RECONCILE OPTIONS:\n  --source PATH         Required source reconciliation snapshot JSON\n  --target PATH         Required target reconciliation snapshot JSON\n  --explanations PATH   Optional JSON object mapping difference keys to explanations\n\n\
All commands reject unknown options, invalid JSON/UTF-8, oversized inputs, unsafe output\n\
paths, and secret-looking fields. No database/network request or external state mutation is\n\
performed.\n";

#[derive(Debug)]
enum CliError {
    Usage(String),
    Io(String),
    InvalidInput(String),
    UnsafePath(String),
    SecretInput,
    Serialization(String),
}

impl std::fmt::Display for CliError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Usage(message)
            | Self::Io(message)
            | Self::InvalidInput(message)
            | Self::UnsafePath(message)
            | Self::Serialization(message) => formatter.write_str(message),
            Self::SecretInput => formatter.write_str("secret-looking input is rejected"),
        }
    }
}

impl std::error::Error for CliError {}

#[derive(Debug)]
enum Command {
    Fixture {
        output: Option<PathBuf>,
    },
    Map {
        namespace: String,
        context: Option<String>,
        input: PathBuf,
        output: Option<PathBuf>,
    },
    Reconcile {
        source: PathBuf,
        target: PathBuf,
        explanations: Option<PathBuf>,
        output: Option<PathBuf>,
    },
    Plan {
        output: Option<PathBuf>,
    },
}

#[tokio::main]
async fn main() {
    match run().await {
        Ok(()) => {}
        Err(CliError::Usage(message)) if message == HELP => {
            println!("{HELP}");
        }
        Err(error) => {
            eprintln!("error: {error}");
            std::process::exit(2);
        }
    }
}

async fn run() -> Result<(), CliError> {
    let args = env::args().skip(1).collect::<Vec<_>>();
    if args.is_empty() || args[0] == "--help" || args[0] == "-h" {
        return Err(CliError::Usage(HELP.to_string()));
    }
    if args[0].starts_with('-') {
        return Err(CliError::Usage(
            "a command is required; use --help for usage".into(),
        ));
    }
    let command = parse_command(&args[0], &args[1..])?;
    let (payload, output) = match command {
        Command::Fixture { output } => (fixture_summary()?, output),
        Command::Map {
            namespace,
            context,
            input,
            output,
        } => (map_ids(&namespace, context.as_deref(), &input)?, output),
        Command::Reconcile {
            source,
            target,
            explanations,
            output,
        } => (
            reconcile(&source, &target, explanations.as_deref())?,
            output,
        ),
        Command::Plan { output } => (execution_plan(), output),
    };
    emit_json(&payload, output.as_deref())
}

fn parse_command(name: &str, args: &[String]) -> Result<Command, CliError> {
    if args
        .iter()
        .any(|argument| argument == "--help" || argument == "-h")
    {
        return Err(CliError::Usage(HELP.to_string()));
    }
    match name {
        "fixture" => Ok(Command::Fixture {
            output: parse_output(args)?,
        }),
        "map" => {
            let mut parser = Arguments::new(args);
            let namespace = parser.required("--namespace")?;
            let context = parser.optional("--context")?;
            let input = PathBuf::from(parser.required("--input")?);
            let output = parser.optional("--output")?.map(PathBuf::from);
            parser.finish()?;
            validate_name("namespace", &namespace)?;
            if let Some(context) = &context {
                validate_name("context", context)?;
            }
            Ok(Command::Map {
                namespace,
                context,
                input,
                output,
            })
        }
        "reconcile" => {
            let mut parser = Arguments::new(args);
            let source = PathBuf::from(parser.required("--source")?);
            let target = PathBuf::from(parser.required("--target")?);
            let explanations = parser.optional("--explanations")?.map(PathBuf::from);
            let output = parser.optional("--output")?.map(PathBuf::from);
            parser.finish()?;
            Ok(Command::Reconcile {
                source,
                target,
                explanations,
                output,
            })
        }
        "plan" => Ok(Command::Plan {
            output: parse_output(args)?,
        }),
        _ => Err(CliError::Usage(format!(
            "unknown command \"{name}\"; use --help for usage"
        ))),
    }
}

fn parse_output(args: &[String]) -> Result<Option<PathBuf>, CliError> {
    let mut parser = Arguments::new(args);
    let output = parser.optional("--output")?.map(PathBuf::from);
    parser.finish()?;
    Ok(output)
}

struct Arguments<'a> {
    args: &'a [String],
    consumed: Vec<bool>,
}

impl<'a> Arguments<'a> {
    fn new(args: &'a [String]) -> Self {
        Self {
            args,
            consumed: vec![false; args.len()],
        }
    }

    fn required(&mut self, option: &str) -> Result<String, CliError> {
        self.optional(option)?
            .ok_or_else(|| CliError::Usage(format!("missing {option}; use --help for usage")))
    }

    fn optional(&mut self, option: &str) -> Result<Option<String>, CliError> {
        let mut found = None;
        for index in 0..self.args.len() {
            if !self.consumed[index] && self.args[index] == option {
                if found.is_some() {
                    return Err(CliError::Usage(format!("duplicate option {option}")));
                }
                let value_index = index + 1;
                if value_index >= self.args.len()
                    || self.consumed[value_index]
                    || self.args[value_index].starts_with('-')
                {
                    return Err(CliError::Usage(format!("{option} requires a value")));
                }
                self.consumed[index] = true;
                self.consumed[value_index] = true;
                found = Some(self.args[value_index].clone());
            }
        }
        Ok(found)
    }

    fn finish(&self) -> Result<(), CliError> {
        if let Some(argument) = self
            .args
            .iter()
            .enumerate()
            .find_map(|(index, argument)| (!self.consumed[index]).then_some(argument))
        {
            return Err(CliError::Usage(format!(
                "unknown option or argument \"{argument}\"; use --help for usage"
            )));
        }
        Ok(())
    }
}

fn validate_name(label: &str, value: &str) -> Result<(), CliError> {
    if value.is_empty() || value.len() > MAX_KEY_BYTES || value.trim() != value {
        return Err(CliError::InvalidInput(format!(
            "{label} is empty, padded, or too long"
        )));
    }
    if looks_secret_name(value) || looks_secret_value(value) {
        return Err(CliError::SecretInput);
    }
    Ok(())
}

fn read_json(path: &Path, label: &str) -> Result<Value, CliError> {
    validate_input_path(path)?;
    let bytes = read_bounded(path)?;
    reject_duplicate_object_keys(&bytes)?;
    parse_json(bytes, label)
}

fn read_mapping_json(path: &Path) -> Result<Value, CliError> {
    validate_input_path(path)?;
    let bytes = read_bounded(path)?;
    reject_duplicate_object_keys(&bytes)?;
    parse_json(bytes, "mapping input")
}

fn parse_json(bytes: Vec<u8>, label: &str) -> Result<Value, CliError> {
    let text = String::from_utf8(bytes)
        .map_err(|_| CliError::InvalidInput(format!("{label} is not valid UTF-8")))?;
    let value: Value = serde_json::from_str(&text)
        .map_err(|_| CliError::InvalidInput(format!("{label} is not valid JSON")))?;
    reject_secrets(&value)?;
    Ok(value)
}

const DUPLICATE_OBJECT_KEY_SENTINEL: &str = "migrator_duplicate_object_key";

struct DuplicateKeySeed;
struct RecursiveDuplicateKeyVisitor;

impl<'de> DeserializeSeed<'de> for DuplicateKeySeed {
    type Value = ();

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(RecursiveDuplicateKeyVisitor)
    }
}

impl<'de> Visitor<'de> for RecursiveDuplicateKeyVisitor {
    type Value = ();

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a JSON value")
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut keys = BTreeSet::new();
        while let Some(key) = map.next_key::<String>()? {
            if !keys.insert(key) {
                return Err(de::Error::custom(DUPLICATE_OBJECT_KEY_SENTINEL));
            }
            map.next_value_seed(DuplicateKeySeed)?;
        }
        Ok(())
    }

    fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        while sequence.next_element_seed(DuplicateKeySeed)?.is_some() {}
        Ok(())
    }

    fn visit_bool<E>(self, _value: bool) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(())
    }

    fn visit_i64<E>(self, _value: i64) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(())
    }

    fn visit_u64<E>(self, _value: u64) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(())
    }

    fn visit_f64<E>(self, _value: f64) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(())
    }

    fn visit_str<E>(self, _value: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(())
    }

    fn visit_string<E>(self, _value: String) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(())
    }

    fn visit_unit<E>(self) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(())
    }
}

fn reject_duplicate_object_keys(bytes: &[u8]) -> Result<(), CliError> {
    struct DuplicateKeyVisitor;

    impl<'de> Visitor<'de> for DuplicateKeyVisitor {
        type Value = ();

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("a JSON value")
        }

        fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
        where
            A: MapAccess<'de>,
        {
            let mut keys = BTreeSet::new();
            while let Some(key) = map.next_key::<String>()? {
                if !keys.insert(key) {
                    return Err(de::Error::custom(DUPLICATE_OBJECT_KEY_SENTINEL));
                }
                map.next_value_seed(DuplicateKeySeed)?;
            }
            Ok(())
        }

        fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
        where
            A: SeqAccess<'de>,
        {
            while sequence.next_element_seed(DuplicateKeySeed)?.is_some() {}
            Ok(())
        }

        fn visit_bool<E>(self, _value: bool) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(())
        }

        fn visit_i64<E>(self, _value: i64) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(())
        }

        fn visit_u64<E>(self, _value: u64) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(())
        }

        fn visit_f64<E>(self, _value: f64) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(())
        }

        fn visit_str<E>(self, _value: &str) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(())
        }

        fn visit_string<E>(self, _value: String) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(())
        }

        fn visit_unit<E>(self) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(())
        }
    }

    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    match deserializer.deserialize_any(DuplicateKeyVisitor) {
        Ok(()) => deserializer
            .end()
            .map_err(|_| CliError::InvalidInput("mapping input is not valid JSON".into())),
        Err(error) if error.to_string().contains(DUPLICATE_OBJECT_KEY_SENTINEL) => Err(
            CliError::InvalidInput("mapping input contains duplicate object keys".into()),
        ),
        Err(_) => Err(CliError::InvalidInput(
            "mapping input is not valid JSON".into(),
        )),
    }
}

fn read_bounded(path: &Path) -> Result<Vec<u8>, CliError> {
    let metadata =
        fs::metadata(path).map_err(|_| CliError::Io("input file cannot be read".into()))?;
    if !metadata.is_file() {
        return Err(CliError::InvalidInput(
            "input path is not a regular file".into(),
        ));
    }
    if metadata.len() > u64::try_from(MAX_JSON_BYTES).unwrap_or(u64::MAX) {
        return Err(CliError::InvalidInput(
            "input file exceeds the bounded size limit".into(),
        ));
    }
    let file =
        fs::File::open(path).map_err(|_| CliError::Io("input file cannot be read".into()))?;
    let mut bytes = Vec::new();
    file.take(u64::try_from(MAX_JSON_BYTES).unwrap_or(u64::MAX) + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| CliError::Io("input file cannot be read".into()))?;
    if bytes.len() > MAX_JSON_BYTES {
        return Err(CliError::InvalidInput(
            "input file exceeds the bounded size limit".into(),
        ));
    }
    Ok(bytes)
}

fn validate_input_path(path: &Path) -> Result<(), CliError> {
    if path.as_os_str().is_empty() || path.is_absolute() || has_parent_component(path) {
        return Err(CliError::UnsafePath("input path is unsafe".into()));
    }
    reject_symlink_components(path)?;
    Ok(())
}

fn validate_output_path(path: &Path) -> Result<(), CliError> {
    if path.as_os_str().is_empty()
        || path.is_absolute()
        || has_parent_component(path)
        || path.file_name().is_none()
    {
        return Err(CliError::UnsafePath("output path is unsafe".into()));
    }
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    if !parent.is_dir() {
        return Err(CliError::UnsafePath(
            "output parent directory does not exist".into(),
        ));
    }
    reject_symlink_components(path)?;
    if let Ok(metadata) = fs::symlink_metadata(path)
        && (metadata.file_type().is_symlink() || metadata.is_dir())
    {
        return Err(CliError::UnsafePath(
            "output path is not a regular file".into(),
        ));
    }
    Ok(())
}

fn has_parent_component(path: &Path) -> bool {
    path.components()
        .any(|component| component == Component::ParentDir)
}

fn reject_symlink_components(path: &Path) -> Result<(), CliError> {
    let mut current = PathBuf::new();
    for component in path.components() {
        current.push(component.as_os_str());
        if let Ok(metadata) = fs::symlink_metadata(&current)
            && metadata.file_type().is_symlink()
        {
            return Err(CliError::UnsafePath(
                "output path contains a symlink".into(),
            ));
        }
    }
    Ok(())
}

fn reject_secrets(value: &Value) -> Result<(), CliError> {
    match value {
        Value::Object(object) => {
            for (key, value) in object {
                if looks_secret_name(key) {
                    return Err(CliError::SecretInput);
                }
                reject_secrets(value)?;
            }
        }
        Value::Array(values) => {
            for value in values {
                reject_secrets(value)?;
            }
        }
        Value::String(string) if looks_secret_value(string) => return Err(CliError::SecretInput),
        _ => {}
    }
    Ok(())
}

fn looks_secret_name(name: &str) -> bool {
    let normalized = name.to_ascii_lowercase().replace(['-', '.'], "_");
    [
        "password",
        "passwd",
        "secret",
        "token",
        "bearer",
        "credential",
        "api_key",
        "apikey",
        "access_key",
        "private_key",
        "database_url",
        "db_url",
        "object_key",
        "connection_string",
        "authorization",
        "cookie",
        "client_secret",
    ]
    .iter()
    .any(|needle| normalized == *needle || normalized.contains(needle))
}

fn looks_secret_value(value: &str) -> bool {
    let normalized = value.to_ascii_lowercase();
    normalized.contains("bearer ")
        || normalized.contains("postgres://")
        || normalized.contains("postgresql://")
        || normalized.contains("mysql://")
        || normalized.contains("redis://")
        || normalized.contains("mongodb://")
        || normalized.contains("-----begin ")
        || normalized.contains("access_token")
        || normalized.contains("client_secret")
        || normalized.contains("authorization:")
        || normalized.contains("x-api-key")
}

#[derive(Serialize)]
struct FixtureSummary {
    mode: &'static str,
    read_only: bool,
    fixture_version: String,
    seed: u64,
    counts: BTreeMap<String, u64>,
    totals_minor: BTreeMap<String, i64>,
}

fn fixture_summary() -> Result<Value, CliError> {
    let fixture = load_deterministic_fixture()
        .map_err(|_| CliError::InvalidInput("deterministic fixture is invalid".into()))?;
    let snapshot = ReconciliationSnapshot::from_fixture(&fixture);
    serde_json::to_value(FixtureSummary {
        mode: "bounded_summary",
        read_only: true,
        fixture_version: fixture.fixture_version,
        seed: fixture.seed,
        counts: snapshot.counts,
        totals_minor: snapshot.totals_minor,
    })
    .map_err(|_| CliError::Serialization("could not serialize fixture summary".into()))
}

#[derive(Deserialize)]
#[serde(transparent)]
struct SourceIds(BTreeMap<String, String>);

#[derive(Serialize)]
struct MappingOutput {
    mode: &'static str,
    read_only: bool,
    namespace: String,
    context: Option<String>,
    mappings: BTreeMap<String, String>,
}

fn map_ids(namespace: &str, context: Option<&str>, path: &Path) -> Result<Value, CliError> {
    let mapper = DeterministicIdMapper::new(namespace)
        .map_err(|_| CliError::InvalidInput("namespace is invalid".into()))?;
    let value = read_mapping_json(path)?;
    let SourceIds(source_ids) = serde_json::from_value(value).map_err(|_| {
        CliError::InvalidInput("mapping input must be an object of UUID strings".into())
    })?;
    if source_ids.is_empty() || source_ids.len() > MAX_MAP_ENTRIES {
        return Err(CliError::InvalidInput(
            "mapping input has an invalid entry count".into(),
        ));
    }
    let mut mappings = BTreeMap::new();
    for (key, source_id) in source_ids {
        validate_key(&key)?;
        if source_id.len() > MAX_KEY_BYTES || looks_secret_value(&source_id) {
            return Err(CliError::SecretInput);
        }
        let source_id = Uuid::parse_str(&source_id).map_err(|_| {
            CliError::InvalidInput("mapping input contains an invalid source UUID".into())
        })?;
        let mapping_context = context.unwrap_or(&key);
        let mapped = mapper.map(mapping_context, source_id);
        if mapped != mapper.map(mapping_context, source_id) {
            return Err(CliError::InvalidInput(
                "deterministic ID mapper returned an unstable result".into(),
            ));
        }
        mappings.insert(key, mapped.to_string());
    }
    validate_mapping_output(&mappings)?;
    serde_json::to_value(MappingOutput {
        mode: "deterministic_mapping",
        read_only: true,
        namespace: namespace.to_string(),
        context: context.map(str::to_string),
        mappings,
    })
    .map_err(|_| CliError::Serialization("could not serialize mapping output".into()))
}

fn validate_mapping_output(mappings: &BTreeMap<String, String>) -> Result<(), CliError> {
    let mut target_owners = BTreeMap::new();
    for (key, target_id) in mappings {
        let canonical_target = Uuid::parse_str(target_id)
            .map_err(|_| {
                CliError::Serialization(
                    "deterministic mapper returned an invalid target UUID".into(),
                )
            })?
            .to_string();
        if target_owners.insert(canonical_target, key).is_some() {
            return Err(CliError::InvalidInput(
                "deterministic mapping contains duplicate target UUIDs".into(),
            ));
        }
    }
    Ok(())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SnapshotInput {
    counts: BTreeMap<String, u64>,
    totals_minor: BTreeMap<String, i64>,
}

#[derive(Serialize)]
struct DifferenceOutput {
    key: String,
    source_count: Option<u64>,
    target_count: Option<u64>,
    source_total_minor: Option<i64>,
    target_total_minor: Option<i64>,
    explained: bool,
    explanation: Option<String>,
}

#[derive(Serialize)]
struct ReconcileOutput {
    mode: &'static str,
    read_only: bool,
    reconciled: bool,
    differences: Vec<DifferenceOutput>,
    unexplained: Vec<String>,
}

fn reconcile(
    source_path: &Path,
    target_path: &Path,
    explanations_path: Option<&Path>,
) -> Result<Value, CliError> {
    let source = parse_snapshot(read_json(source_path, "source snapshot")?)?;
    let target = parse_snapshot(read_json(target_path, "target snapshot")?)?;
    let explanations = explanations_path
        .map(|path| parse_explanations(read_json(path, "explanations")?))
        .transpose()?
        .unwrap_or_default();
    let report = reconcile_snapshots(&source, &target, &explanations);
    let differences = report
        .differences
        .into_iter()
        .map(|difference| DifferenceOutput {
            key: difference.key,
            source_count: difference.source_count,
            target_count: difference.target_count,
            source_total_minor: difference.source_total_minor,
            target_total_minor: difference.target_total_minor,
            explained: difference.explanation.is_some(),
            explanation: difference.explanation,
        })
        .collect::<Vec<_>>();
    let reconciled = differences.is_empty() && report.unexplained.is_empty();
    serde_json::to_value(ReconcileOutput {
        mode: "reconciliation_report",
        read_only: true,
        reconciled,
        differences,
        unexplained: report.unexplained,
    })
    .map_err(|_| CliError::Serialization("could not serialize reconciliation report".into()))
}

fn parse_snapshot(value: Value) -> Result<ReconciliationSnapshot, CliError> {
    let SnapshotInput {
        counts,
        totals_minor,
    } = serde_json::from_value(value).map_err(|_| {
        CliError::InvalidInput("snapshot must contain only counts and totals_minor maps".into())
    })?;
    validate_metric_map(&counts)?;
    validate_metric_map(&totals_minor)?;
    if counts.len() + totals_minor.len() > MAX_SNAPSHOT_ENTRIES {
        return Err(CliError::InvalidInput(
            "snapshot has too many metric entries".into(),
        ));
    }
    Ok(ReconciliationSnapshot {
        counts,
        totals_minor,
    })
}

fn parse_explanations(value: Value) -> Result<BTreeMap<String, String>, CliError> {
    let explanations: BTreeMap<String, String> = serde_json::from_value(value)
        .map_err(|_| CliError::InvalidInput("explanations must be an object of strings".into()))?;
    if explanations.len() > MAX_EXPLANATIONS {
        return Err(CliError::InvalidInput("too many explanations".into()));
    }
    for (key, explanation) in &explanations {
        validate_key(key)?;
        if explanation.is_empty() || explanation.len() > MAX_EXPLANATION_BYTES {
            return Err(CliError::InvalidInput(
                "explanation is empty or too long".into(),
            ));
        }
    }
    Ok(explanations)
}

fn validate_metric_map<T>(metrics: &BTreeMap<String, T>) -> Result<(), CliError> {
    for key in metrics.keys() {
        validate_key(key)?;
    }
    Ok(())
}

fn validate_key(key: &str) -> Result<(), CliError> {
    if key.is_empty() || key.len() > MAX_KEY_BYTES || key.trim() != key {
        return Err(CliError::InvalidInput(
            "a JSON key is empty, padded, or too long".into(),
        ));
    }
    if looks_secret_name(key) {
        return Err(CliError::SecretInput);
    }
    Ok(())
}

#[derive(Serialize)]
struct PlanStep {
    id: &'static str,
    action: &'static str,
    owner: &'static str,
    requires: &'static [&'static str],
    evidence: &'static str,
    performed: bool,
    state_change: bool,
    rollback_safe: bool,
}

#[derive(Serialize)]
struct ExecutionPlan {
    mode: &'static str,
    read_only: bool,
    state_change: bool,
    steps: Vec<PlanStep>,
}

type PlanDefinition = (
    &'static str,
    &'static str,
    &'static str,
    &'static [&'static str],
    &'static str,
    bool,
    bool,
    bool,
);

fn execution_plan() -> Value {
    let definitions: [PlanDefinition; 12] = [
        (
            "freeze_source",
            "freeze source",
            "source-owner",
            &[],
            "source_contract_hash",
            false,
            true,
            true,
        ),
        (
            "map_ids",
            "map IDs",
            "migration-owner",
            &["freeze_source"],
            "id_mapping_digest",
            false,
            false,
            true,
        ),
        (
            "backfill",
            "backfill",
            "migration-owner",
            &["map_ids"],
            "target_import_checkpoint",
            false,
            true,
            true,
        ),
        (
            "project",
            "project",
            "projection-owner",
            &["backfill"],
            "read_model_checkpoint",
            false,
            true,
            true,
        ),
        (
            "shadow_replay",
            "shadow replay",
            "replay-owner",
            &["project"],
            "replay_checkpoint",
            false,
            false,
            true,
        ),
        (
            "compare",
            "compare",
            "reconciliation-owner",
            &["shadow_replay"],
            "reconciliation_report",
            false,
            false,
            true,
        ),
        (
            "explain",
            "explain",
            "reconciliation-owner",
            &["compare"],
            "reconciliation_explanations",
            false,
            false,
            true,
        ),
        (
            "pilot",
            "pilot",
            "cutover-owner",
            &["explain"],
            "pilot_read_only_evidence",
            false,
            true,
            true,
        ),
        (
            "rollback_rehearsal",
            "rollback rehearsal",
            "cutover-owner",
            &["pilot"],
            "rollback_rehearsal_evidence",
            false,
            true,
            true,
        ),
        (
            "final_delta",
            "final delta",
            "migration-owner",
            &["rollback_rehearsal"],
            "final_delta_checkpoint",
            false,
            true,
            true,
        ),
        (
            "command_owner_switch",
            "command-owner switch",
            "command-owner",
            &["final_delta"],
            "cutover_approval",
            false,
            true,
            false,
        ),
        (
            "legacy_read_only_retention",
            "Legacy read-only retention",
            "legacy-owner",
            &["command_owner_switch"],
            "retention_evidence",
            false,
            true,
            false,
        ),
    ];
    let steps = definitions
        .into_iter()
        .map(
            |(id, action, owner, requires, evidence, performed, state_change, rollback_safe)| {
                PlanStep {
                    id,
                    action,
                    owner,
                    requires,
                    evidence,
                    performed,
                    state_change,
                    rollback_safe,
                }
            },
        )
        .collect::<Vec<_>>();
    validate_execution_plan(&steps).expect("static execution plan must remain dependency-safe");
    serde_json::to_value(ExecutionPlan {
        mode: "dry_run_execution_plan",
        read_only: true,
        state_change: false,
        steps,
    })
    .expect("static execution plan serializes")
}

fn validate_execution_plan(steps: &[PlanStep]) -> Result<(), &'static str> {
    let mut seen = BTreeSet::new();
    for step in steps {
        if !seen.insert(step.id) {
            return Err("execution plan contains duplicate step IDs");
        }
        if step.evidence.is_empty() {
            return Err("execution plan step is missing its evidence requirement");
        }
        if !step.rollback_safe
            && !step.requires.contains(&"final_delta")
            && step.id != "legacy_read_only_retention"
        {
            return Err("non-rollback-safe step is not gated by final delta");
        }
        for dependency in step.requires {
            if !seen.contains(dependency) {
                return Err("execution plan dependency is missing or out of order");
            }
        }
    }
    Ok(())
}

fn emit_json(value: &Value, output: Option<&Path>) -> Result<(), CliError> {
    reject_secrets(value)?;
    let serialized = serde_json::to_string_pretty(value)
        .map_err(|_| CliError::Serialization("could not serialize JSON output".into()))?;
    if let Some(path) = output {
        validate_output_path(path)?;
        fs::write(path, format!("{serialized}\n"))
            .map_err(|_| CliError::Io("output file cannot be written".into()))?;
    } else {
        use std::io::Write;
        let mut stdout = io::BufWriter::new(io::stdout().lock());
        writeln!(stdout, "{serialized}")
            .map_err(|_| CliError::Io("stdout cannot be written".into()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mapping_is_deterministic_and_context_bound() {
        let input = PathBuf::from("test-map-input.json");
        fs::write(
            &input,
            r#"{"customer":"00000000-0000-0000-0000-000000000001"}"#,
        )
        .expect("write input");
        let first = map_ids("test-namespace", None, &input).expect("map");
        let second = map_ids("test-namespace", None, &input).expect("map");
        let other_context = map_ids("test-namespace", Some("other"), &input).expect("map");
        assert_eq!(first, second);
        assert_ne!(first, other_context);
        fs::remove_file(input).expect("remove input");
    }

    #[test]
    fn mapping_rejects_duplicate_source_keys() {
        let input = PathBuf::from("test-map-duplicate-input.json");
        fs::write(
            &input,
            r#"{"customer":"00000000-0000-0000-0000-000000000001","customer":"00000000-0000-0000-0000-000000000002"}"#,
        )
        .expect("write input");
        let error = map_ids("test-namespace", None, &input).expect_err("duplicate key");
        assert_eq!(
            error.to_string(),
            "mapping input contains duplicate object keys"
        );
        fs::remove_file(input).expect("remove input");
    }

    #[test]
    fn mapping_rejects_duplicate_target_uuids() {
        let mappings = BTreeMap::from([
            (
                "customer".to_string(),
                "00000000-0000-0000-0000-000000000001".to_string(),
            ),
            (
                "order".to_string(),
                "00000000-0000-0000-0000-000000000001".to_string(),
            ),
        ]);
        let error = validate_mapping_output(&mappings).expect_err("target collision");
        assert_eq!(
            error.to_string(),
            "deterministic mapping contains duplicate target UUIDs"
        );
    }

    #[test]
    fn explanation_is_reported_without_marking_other_differences_explained() {
        let source = serde_json::json!({"counts":{"orders":2,"customers":1},"totals_minor":{}});
        let target = serde_json::json!({"counts":{"orders":3,"customers":2},"totals_minor":{}});
        let explanation = serde_json::json!({"orders":"late source event"});
        let dir = PathBuf::from("test-reconcile-inputs");
        fs::create_dir_all(&dir).expect("create directory");
        fs::write(dir.join("source.json"), source.to_string()).expect("source");
        fs::write(dir.join("target.json"), target.to_string()).expect("target");
        fs::write(dir.join("explanations.json"), explanation.to_string()).expect("explanation");
        let report = reconcile(
            &dir.join("source.json"),
            &dir.join("target.json"),
            Some(&dir.join("explanations.json")),
        )
        .expect("reconcile");
        let differences = report["differences"].as_array().expect("differences");
        assert!(
            differences
                .iter()
                .any(|difference| difference["key"] == "orders" && difference["explained"] == true)
        );
        assert!(
            differences
                .iter()
                .any(|difference| difference["key"] == "customers"
                    && difference["explained"] == false)
        );
        fs::remove_dir_all(dir).expect("remove directory");
    }

    #[test]
    fn reconciliation_inputs_reject_duplicate_object_keys() {
        let dir = PathBuf::from("test-reconcile-duplicate-inputs");
        fs::create_dir_all(&dir).expect("create directory");
        fs::write(
            dir.join("source.json"),
            r#"{"counts":{"orders":1,"orders":2},"totals_minor":{}}"#,
        )
        .expect("source");
        fs::write(
            dir.join("target.json"),
            r#"{"counts":{"orders":2},"totals_minor":{}}"#,
        )
        .expect("target");

        let error = reconcile(&dir.join("source.json"), &dir.join("target.json"), None)
            .expect_err("duplicate source keys must be rejected");
        assert_eq!(
            error.to_string(),
            "mapping input contains duplicate object keys"
        );
        fs::remove_dir_all(dir).expect("cleanup");
    }

    #[test]
    fn unsafe_and_secret_inputs_are_rejected_without_echoing_values() {
        assert!(validate_input_path(Path::new("/tmp/secret.json")).is_err());
        assert!(validate_output_path(Path::new("../unsafe.json")).is_err());
        assert!(validate_output_path(Path::new("/tmp/unsafe.json")).is_err());
        let value = serde_json::json!({"api_token":"do-not-print-this"});
        let error = reject_secrets(&value).expect_err("secret should be rejected");
        assert_eq!(error.to_string(), "secret-looking input is rejected");
        assert!(!error.to_string().contains("do-not-print-this"));
    }

    #[test]
    fn execution_plan_exposes_ordered_evidence_and_rollback_guards() {
        let plan = execution_plan();
        let steps = plan["steps"].as_array().expect("steps");
        let index = |id: &str| {
            steps
                .iter()
                .position(|step| step["id"] == id)
                .expect("step")
        };
        for (step, dependency) in [
            ("map_ids", "freeze_source"),
            ("backfill", "map_ids"),
            ("project", "backfill"),
            ("shadow_replay", "project"),
            ("compare", "shadow_replay"),
            ("rollback_rehearsal", "pilot"),
            ("final_delta", "rollback_rehearsal"),
            ("command_owner_switch", "final_delta"),
        ] {
            assert!(index(dependency) < index(step));
            assert!(
                steps[index(step)]["requires"]
                    .as_array()
                    .expect("requires")
                    .iter()
                    .any(|required| required == dependency)
            );
        }
        for step in steps {
            assert!(
                step["evidence"]
                    .as_str()
                    .is_some_and(|evidence| !evidence.is_empty())
            );
            assert_eq!(step["performed"], false);
        }
        assert_eq!(steps[index("rollback_rehearsal")]["rollback_safe"], true);
        assert_eq!(steps[index("command_owner_switch")]["rollback_safe"], false);
    }

    #[test]
    fn execution_plan_validator_rejects_out_of_order_dependencies() {
        let steps = [PlanStep {
            id: "target",
            action: "target",
            owner: "test",
            requires: &["missing"],
            evidence: "evidence",
            performed: false,
            state_change: false,
            rollback_safe: true,
        }];
        assert_eq!(
            validate_execution_plan(&steps),
            Err("execution plan dependency is missing or out of order")
        );
    }
}
