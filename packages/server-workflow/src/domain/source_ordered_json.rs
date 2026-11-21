//! Scoped iterative ECMAScript JSON codec. It grants no command authority.
//! Memory is proportional to input; callers own ingress and concurrency limits.
use erpx_server_shared::public::BusinessError;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
fn invalid() -> BusinessError {
    BusinessError::invalid("SOURCE_ORDERED_JSON_INVALID", "Provide a valid JSON value.")
}
#[derive(Clone)]
enum Node {
    Null,
    Bool(bool),
    Num(f64),
    Str(Vec<u16>),
    Arr(Vec<usize>),
    Obj(Vec<(Vec<u16>, usize)>),
}
enum Frame {
    Array {
        node: usize,
        state: u8,
    },
    Object {
        node: usize,
        state: u8,
        key: Vec<u16>,
        positions: HashMap<Vec<u16>, usize>,
    },
}
pub struct SourceOrderedJson {
    nodes: Vec<Node>,
    root: usize,
}
struct Parser<'a> {
    s: &'a str,
    i: usize,
    nodes: Vec<Node>,
    stack: Vec<Frame>,
}
impl Parser<'_> {
    fn peek(&self) -> Option<u8> {
        self.s.as_bytes().get(self.i).copied()
    }
    fn ws(&mut self) {
        while self
            .peek()
            .is_some_and(|b| matches!(b, b' ' | b'\n' | b'\r' | b'\t'))
        {
            self.i += 1
        }
    }
    fn string(&mut self) -> Result<Vec<u16>, BusinessError> {
        if self.peek() != Some(b'"') {
            return Err(invalid());
        }
        self.i += 1;
        let mut out = Vec::new();
        loop {
            match self.peek().ok_or_else(invalid)? {
                b'"' => {
                    self.i += 1;
                    return Ok(out);
                }
                b'\\' => {
                    self.i += 1;
                    let c = self.peek().ok_or_else(invalid)?;
                    self.i += 1;
                    match c {
                        b'"' | b'\\' | b'/' => out.push(c as u16),
                        b'b' => out.push(8),
                        b'f' => out.push(12),
                        b'n' => out.push(10),
                        b'r' => out.push(13),
                        b't' => out.push(9),
                        b'u' => {
                            let end = self.i.checked_add(4).ok_or_else(invalid)?;
                            let t = self.s.get(self.i..end).ok_or_else(invalid)?;
                            if !t.bytes().all(|b| b.is_ascii_hexdigit()) {
                                return Err(invalid());
                            }
                            out.push(u16::from_str_radix(t, 16).map_err(|_| invalid())?);
                            self.i = end
                        }
                        _ => return Err(invalid()),
                    }
                }
                0..=31 => return Err(invalid()),
                _ => {
                    let c = self.s[self.i..].chars().next().ok_or_else(invalid)?;
                    self.i += c.len_utf8();
                    let mut x = [0; 2];
                    out.extend_from_slice(c.encode_utf16(&mut x));
                }
            }
        }
    }
    fn value(&mut self) -> Result<usize, BusinessError> {
        self.ws();
        let node = match self.peek().ok_or_else(invalid)? {
            b'n' if self.s[self.i..].starts_with("null") => {
                self.i += 4;
                Node::Null
            }
            b't' if self.s[self.i..].starts_with("true") => {
                self.i += 4;
                Node::Bool(true)
            }
            b'f' if self.s[self.i..].starts_with("false") => {
                self.i += 5;
                Node::Bool(false)
            }
            b'"' => Node::Str(self.string()?),
            b'[' => {
                self.i += 1;
                Node::Arr(Vec::new())
            }
            b'{' => {
                self.i += 1;
                Node::Obj(Vec::new())
            }
            b'-' | b'0'..=b'9' => {
                let start = self.i;
                if self.peek() == Some(b'-') {
                    self.i += 1
                }
                match self.peek() {
                    Some(b'0') => self.i += 1,
                    Some(b'1'..=b'9') => {
                        self.i += 1;
                        while self.peek().is_some_and(|x| x.is_ascii_digit()) {
                            self.i += 1
                        }
                    }
                    _ => return Err(invalid()),
                }
                if self.peek() == Some(b'.') {
                    self.i += 1;
                    self.digits()?
                }
                if self.peek().is_some_and(|b| matches!(b, b'e' | b'E')) {
                    self.i += 1;
                    if self.peek().is_some_and(|b| matches!(b, b'+' | b'-')) {
                        self.i += 1
                    }
                    self.digits()?
                }
                Node::Num(self.s[start..self.i].parse().map_err(|_| invalid())?)
            }
            _ => return Err(invalid()),
        };
        let index = self.nodes.len();
        let frame = match &node {
            Node::Arr(_) => Some(Frame::Array {
                node: index,
                state: 0,
            }),
            Node::Obj(_) => Some(Frame::Object {
                node: index,
                state: 0,
                key: Vec::new(),
                positions: HashMap::new(),
            }),
            _ => None,
        };
        self.nodes.push(node);
        if let Some(f) = frame {
            self.stack.push(f)
        }
        Ok(index)
    }
    fn digits(&mut self) -> Result<(), BusinessError> {
        let start = self.i;
        while self.peek().is_some_and(|b| b.is_ascii_digit()) {
            self.i += 1
        }
        if start == self.i {
            Err(invalid())
        } else {
            Ok(())
        }
    }
    fn parse(mut self) -> Result<SourceOrderedJson, BusinessError> {
        let root = self.value()?;
        while let Some(frame) = self.stack.pop() {
            self.ws();
            match frame {
                Frame::Array { node, state } => match state {
                    0 | 1 => {
                        if state == 0 && self.peek() == Some(b']') {
                            self.i += 1;
                            continue;
                        }
                        let prior = self.stack.len();
                        let child = self.value()?;
                        if let Node::Arr(ref mut a) = self.nodes[node] {
                            a.push(child)
                        }
                        let child_frame = if self.stack.len() > prior {
                            self.stack.pop()
                        } else {
                            None
                        };
                        self.stack.push(Frame::Array { node, state: 2 });
                        if let Some(f) = child_frame {
                            self.stack.push(f)
                        }
                    }
                    2 => match self.peek() {
                        Some(b',') => {
                            self.i += 1;
                            self.stack.push(Frame::Array { node, state: 1 })
                        }
                        Some(b']') => self.i += 1,
                        _ => return Err(invalid()),
                    },
                    _ => return Err(invalid()),
                },
                Frame::Object {
                    node,
                    state,
                    key,
                    mut positions,
                } => match state {
                    0 | 1 => {
                        if state == 0 && self.peek() == Some(b'}') {
                            self.i += 1;
                            continue;
                        }
                        let key = self.string()?;
                        self.ws();
                        if self.peek() != Some(b':') {
                            return Err(invalid());
                        }
                        self.i += 1;
                        self.stack.push(Frame::Object {
                            node,
                            state: 2,
                            key,
                            positions,
                        })
                    }
                    2 => {
                        let prior = self.stack.len();
                        let child = self.value()?;
                        if let Node::Obj(ref mut o) = self.nodes[node] {
                            if let Some(i) = positions.get(&key) {
                                o[*i].1 = child
                            } else {
                                positions.insert(key.clone(), o.len());
                                o.push((key, child))
                            }
                        }
                        let child_frame = if self.stack.len() > prior {
                            self.stack.pop()
                        } else {
                            None
                        };
                        self.stack.push(Frame::Object {
                            node,
                            state: 3,
                            key: Vec::new(),
                            positions,
                        });
                        if let Some(f) = child_frame {
                            self.stack.push(f)
                        }
                    }
                    3 => match self.peek() {
                        Some(b',') => {
                            self.i += 1;
                            self.stack.push(Frame::Object {
                                node,
                                state: 1,
                                key: Vec::new(),
                                positions,
                            })
                        }
                        Some(b'}') => self.i += 1,
                        _ => return Err(invalid()),
                    },
                    _ => return Err(invalid()),
                },
            }
        }
        self.ws();
        if self.i != self.s.len() {
            return Err(invalid());
        }
        Ok(SourceOrderedJson {
            nodes: self.nodes,
            root,
        })
    }
}
fn string(out: &mut impl std::fmt::Write, units: &[u16]) {
    out.write_char('"').expect("infallible sink");
    for c in char::decode_utf16(units.iter().copied()) {
        match c {
            Ok(c) => {
                let x = serde_json::to_string(&c.to_string()).expect("char encoding");
                out.write_str(&x[1..x.len() - 1]).expect("infallible sink");
            }
            Err(e) => {
                write!(out, "\\u{:04x}", e.unpaired_surrogate()).expect("infallible sink");
            }
        }
    }
    out.write_char('"').expect("infallible sink")
}
fn index(units: &[u16]) -> Option<u32> {
    let s = String::from_utf16(units).ok()?;
    let n = s.parse::<u32>().ok()?;
    (n < u32::MAX && n.to_string() == s).then_some(n)
}
enum WriteOp<'a> {
    Value(usize),
    Text(&'static str),
    Key(&'a [u16]),
}
impl SourceOrderedJson {
    pub fn parse(s: &str) -> Result<Self, BusinessError> {
        Parser {
            s,
            i: 0,
            nodes: Vec::new(),
            stack: Vec::new(),
        }
        .parse()
    }
    fn write_to(&self, out: &mut impl std::fmt::Write) {
        self.write_node(self.root, out);
    }
    fn write_node(&self, root: usize, out: &mut impl std::fmt::Write) {
        let mut work = vec![WriteOp::Value(root)];
        while let Some(op) = work.pop() {
            match op {
                WriteOp::Text(s) => out.write_str(s).expect("infallible sink"),
                WriteOp::Key(k) => string(out, k),
                WriteOp::Value(i) => match &self.nodes[i] {
                    Node::Null => out.write_str("null").unwrap(),
                    Node::Bool(b) => out.write_str(if *b { "true" } else { "false" }).unwrap(),
                    Node::Num(n) => {
                        if !n.is_finite() {
                            out.write_str("null").unwrap()
                        } else if *n == 0.0 {
                            out.write_char('0').unwrap()
                        } else {
                            out.write_str(ryu_js::Buffer::new().format_finite(*n))
                                .unwrap()
                        }
                    }
                    Node::Str(s) => string(out, s),
                    Node::Arr(items) => {
                        out.write_char('[').unwrap();
                        work.push(WriteOp::Text("]"));
                        for (j, v) in items.iter().enumerate().rev() {
                            work.push(WriteOp::Value(*v));
                            if j > 0 {
                                work.push(WriteOp::Text(","))
                            }
                        }
                    }
                    Node::Obj(items) => {
                        out.write_char('{').unwrap();
                        let mut keys: Vec<usize> = (0..items.len()).collect();
                        keys.sort_by_key(|&i| {
                            index(&items[i].0)
                                .map(|n| (0, n as usize))
                                .unwrap_or((1, i))
                        });
                        work.push(WriteOp::Text("}"));
                        for (position, &i) in keys.iter().enumerate().rev() {
                            work.push(WriteOp::Value(items[i].1));
                            work.push(WriteOp::Text(":"));
                            work.push(WriteOp::Key(&items[i].0));
                            if position > 0 {
                                work.push(WriteOp::Text(","))
                            }
                        }
                    }
                },
            }
        }
    }
    pub fn stringify(&self) -> String {
        let mut out = String::new();
        self.write_to(&mut out);
        out
    }
    pub fn sha256(&self) -> String {
        struct Sink(Sha256);
        impl std::fmt::Write for Sink {
            fn write_str(&mut self, s: &str) -> std::fmt::Result {
                self.0.update(s.as_bytes());
                Ok(())
            }
        }
        let mut sink = Sink(Sha256::new());
        self.write_to(&mut sink);
        hex::encode(sink.0.finalize())
    }
}

/// Borrowed view over a node; does not clone large subtrees or grant authority.
#[derive(Clone, Copy)]
pub struct SourceOrderedValue<'a> {
    source: &'a SourceOrderedJson,
    index: usize,
}
impl SourceOrderedJson {
    pub fn root(&self) -> SourceOrderedValue<'_> {
        SourceOrderedValue {
            source: self,
            index: self.root,
        }
    }
}
impl<'a> SourceOrderedValue<'a> {
    pub fn property(&self, key: &str) -> Option<Self> {
        let units: Vec<u16> = key.encode_utf16().collect();
        self.property_utf16(&units)
    }
    /// Borrow an object property by exact JavaScript UTF-16 key, including an
    /// unpaired surrogate. This does not walk a prototype chain.
    pub fn property_utf16(&self, units: &[u16]) -> Option<Self> {
        match &self.source.nodes[self.index] {
            Node::Obj(fields) => fields.iter().find(|(k, _)| k == units).map(|(_, i)| Self {
                source: self.source,
                index: *i,
            }),
            _ => None,
        }
    }
    pub fn kind(&self) -> &'static str {
        match &self.source.nodes[self.index] {
            Node::Null => "null",
            Node::Bool(_) => "boolean",
            Node::Num(_) => "number",
            Node::Str(_) => "string",
            Node::Arr(_) => "array",
            Node::Obj(_) => "object",
        }
    }
    pub fn is_object(&self) -> bool {
        matches!(self.source.nodes[self.index], Node::Obj(_))
    }
    /// Borrow every own JSON object property in insertion order. Serialization
    /// still applies ECMAScript integer-key enumeration order.
    pub fn object_fields(&self) -> Option<Vec<(&'a [u16], Self)>> {
        match &self.source.nodes[self.index] {
            Node::Obj(fields) => Some(
                fields
                    .iter()
                    .map(|(key, index)| {
                        (
                            key.as_slice(),
                            Self {
                                source: self.source,
                                index: *index,
                            },
                        )
                    })
                    .collect(),
            ),
            _ => None,
        }
    }
    /// Borrow an array element without reserializing or losing UTF-16 data.
    /// Non-arrays and absent indices return None; this is not general JS
    /// property access on boxed primitives or arbitrary string keys.
    pub fn element(&self, index: usize) -> Option<Self> {
        match &self.source.nodes[self.index] {
            Node::Arr(items) => items.get(index).map(|&index| Self {
                source: self.source,
                index,
            }),
            _ => None,
        }
    }
    pub fn array_len(&self) -> Option<usize> {
        match &self.source.nodes[self.index] {
            Node::Arr(items) => Some(items.len()),
            _ => None,
        }
    }
    /// ECMAScript ToBoolean for a value obtained from JSON.parse.
    pub fn truthy(&self) -> bool {
        match &self.source.nodes[self.index] {
            Node::Null => false,
            Node::Bool(value) => *value,
            Node::Num(value) => *value != 0.0 && !value.is_nan(),
            Node::Str(value) => !value.is_empty(),
            Node::Arr(_) | Node::Obj(_) => true,
        }
    }
    pub fn utf16(&self) -> Option<&'a [u16]> {
        match &self.source.nodes[self.index] {
            Node::Str(v) => Some(v),
            _ => None,
        }
    }
    pub fn number(&self) -> Option<f64> {
        match self.source.nodes[self.index] {
            Node::Num(v) => Some(v),
            _ => None,
        }
    }
    pub fn boolean(&self) -> Option<bool> {
        match self.source.nodes[self.index] {
            Node::Bool(v) => Some(v),
            _ => None,
        }
    }
    pub fn stringify(&self) -> String {
        let mut s = String::new();
        self.source.write_node(self.index, &mut s);
        s
    }
}
/// Distinct keys are required. Values remain borrowed from their input arena.
pub enum SourceOrderedObjectValue<'a> {
    Borrowed(SourceOrderedValue<'a>),
    Utf16(Vec<u16>),
    Number(f64),
}
pub struct SourceOrderedObject<'a> {
    fields: Vec<(Vec<u16>, SourceOrderedObjectValue<'a>)>,
    keys: HashSet<Vec<u16>>,
}
impl<'a> SourceOrderedObject<'a> {
    pub fn new() -> Self {
        Self {
            fields: Vec::new(),
            keys: HashSet::new(),
        }
    }
    pub fn push(
        &mut self,
        key: &str,
        value: SourceOrderedObjectValue<'a>,
    ) -> Result<(), BusinessError> {
        self.push_utf16(key.encode_utf16().collect(), value)
    }
    pub fn push_utf16(
        &mut self,
        key: Vec<u16>,
        value: SourceOrderedObjectValue<'a>,
    ) -> Result<(), BusinessError> {
        if !self.keys.insert(key.clone()) {
            return Err(invalid());
        }
        self.fields.push((key, value));
        Ok(())
    }
    pub fn stringify(&self) -> String {
        let mut out = String::from("{");
        let mut keys: Vec<_> = (0..self.fields.len()).collect();
        keys.sort_by_key(|&i| {
            index(&self.fields[i].0)
                .map(|n| (0, n as usize))
                .unwrap_or((1, i))
        });
        for (position, i) in keys.into_iter().enumerate() {
            if position > 0 {
                out.push(',')
            }
            let (key, value) = &self.fields[i];
            string(&mut out, key);
            out.push(':');
            match value {
                SourceOrderedObjectValue::Borrowed(v) => v.source.write_node(v.index, &mut out),
                SourceOrderedObjectValue::Utf16(v) => string(&mut out, v),
                SourceOrderedObjectValue::Number(v) => {
                    if !v.is_finite() {
                        out.push_str("null");
                    } else if *v == 0.0 {
                        out.push('0');
                    } else {
                        out.push_str(ryu_js::Buffer::new().format_finite(*v));
                    }
                }
            }
        }
        out.push('}');
        out
    }
}
impl Default for SourceOrderedObject<'_> {
    fn default() -> Self {
        Self::new()
    }
}

/// Fingerprint metadata in the retained Workflow property order. A
/// `CommandActor` is metadata, not proof of authority: composition must obtain
/// it from the current Identity owner and validate it in the protected unit.
pub fn source_ordered_command_fingerprint(
    actor: &crate::application::commands::CommandActor,
    operation: &str,
    record_id: &str,
    input: &SourceOrderedJson,
) -> Result<String, BusinessError> {
    use crate::application::commands::CommandField;
    actor.validate()?;
    let branch = match &actor.branch_id {
        CommandField::Omitted => CommandField::Omitted,
        CommandField::Null => CommandField::Null,
        CommandField::Value(v) => CommandField::Value(v.as_str()),
    };
    let department = match &actor.department_id {
        CommandField::Omitted => CommandField::Omitted,
        CommandField::Null => CommandField::Null,
        CommandField::Value(v) => CommandField::Value(v.as_str()),
    };
    let authority = match &actor.authority {
        CommandField::Omitted => CommandField::Omitted,
        CommandField::Null => CommandField::Null,
        CommandField::Value(a) => CommandField::Value(ScopedWorkAuthority {
            session_id: &a.session_id,
            revision: a.revision,
            assignment_version: a.assignment_version,
        }),
    };
    Ok(source_ordered_fingerprint_metadata(
        &SourceOrderedFingerprintActor {
            user_id: actor.user_id.as_str(),
            assignment_id: actor.assignment_id.as_str(),
            role: &actor.role,
            company_id: actor.company_id.as_str(),
            branch,
            department,
            authority,
        },
        operation,
        record_id,
        input,
    ))
}

/// Hash inputs only: never proof of a session, role, scope or permission.
pub struct ScopedWorkAuthority<'a> {
    pub session_id: &'a str,
    pub revision: i64,
    pub assignment_version: i64,
}
/// Caller must derive these fields from current protected Identity context;
/// this value is not deserializable from a request and grants no authority.
pub struct SourceOrderedFingerprintActor<'a> {
    pub user_id: &'a str,
    pub assignment_id: &'a str,
    pub role: &'a str,
    pub company_id: &'a str,
    pub branch: crate::application::commands::CommandField<&'a str>,
    pub department: crate::application::commands::CommandField<&'a str>,
    pub authority: crate::application::commands::CommandField<ScopedWorkAuthority<'a>>,
}
pub fn source_ordered_fingerprint_metadata(
    actor: &SourceOrderedFingerprintActor<'_>,
    operation: &str,
    record_id: &str,
    input: &SourceOrderedJson,
) -> String {
    use crate::application::commands::CommandField;
    use std::fmt::Write;
    struct Sink(Sha256);
    impl std::fmt::Write for Sink {
        fn write_str(&mut self, s: &str) -> std::fmt::Result {
            self.0.update(s.as_bytes());
            Ok(())
        }
    }
    fn text(out: &mut impl std::fmt::Write, value: &str) {
        string(out, &value.encode_utf16().collect::<Vec<_>>());
    }
    fn field(out: &mut impl std::fmt::Write, name: &str, value: &CommandField<&str>) {
        match value {
            CommandField::Omitted => {}
            CommandField::Null => {
                write!(out, ",\"{name}\":null").unwrap();
            }
            CommandField::Value(v) => {
                write!(out, ",\"{name}\":").unwrap();
                text(out, v);
            }
        }
    }
    let mut out = Sink(Sha256::new());
    out.write_str("{\"actor\":").unwrap();
    text(&mut out, actor.user_id);
    out.write_str(",\"assignment\":").unwrap();
    text(&mut out, actor.assignment_id);
    out.write_str(",\"role\":").unwrap();
    text(&mut out, &actor.role);
    match &actor.authority {
        CommandField::Omitted => {}
        CommandField::Null => out.write_str(",\"authority\":null").unwrap(),
        CommandField::Value(a) => {
            out.write_str(",\"authority\":{\"sessionId\":").unwrap();
            text(&mut out, &a.session_id);
            write!(
                out,
                ",\"revision\":{},\"assignmentVersion\":{}}}",
                ryu_js::Buffer::new().format_finite(a.revision as f64),
                ryu_js::Buffer::new().format_finite(a.assignment_version as f64)
            )
            .unwrap();
        }
    }
    out.write_str(",\"company\":").unwrap();
    text(&mut out, actor.company_id);
    field(&mut out, "branch", &actor.branch);
    field(&mut out, "department", &actor.department);
    out.write_str(",\"operation\":").unwrap();
    text(&mut out, operation);
    out.write_str(",\"recordId\":").unwrap();
    text(&mut out, record_id);
    out.write_str(",\"input\":").unwrap();
    input.write_to(&mut out);
    out.write_char('}').unwrap();
    hex::encode(out.0.finalize())
}
