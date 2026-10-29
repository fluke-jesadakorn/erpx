//! Bounded OPC/WordML raw-text adapter. ZIP/XML decoding remains in vetted
//! libraries; the extraction rules preserve the independent Mammoth B0 oracle.
use crate::domain::document_extraction::{
    DOCUMENT_INPUT_LIMIT, DocumentExtractionFailure as Failure, EXTRACTION_TEXT_LIMIT,
};
use roxmltree::{Document, Node, ParsingOptions};
use std::{
    collections::HashSet,
    io::{Cursor, Read},
};
mod dingbats;

const EXPANDED_LIMIT: u64 = 32 * 1024 * 1024;
const ENTRY_LIMIT: usize = 4096;
const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const W_STRICT: &str = "http://purl.oclc.org/ooxml/wordprocessingml/main";
const MC: &str = "http://schemas.openxmlformats.org/markup-compatibility/2006";
const VML: &str = "urn:schemas-microsoft-com:vml";
const WORD2010: &str = "http://schemas.microsoft.com/office/word/2010/wordml";

pub fn extract_docx_text(bytes: &[u8]) -> Result<String, Failure> {
    if bytes.len() > DOCUMENT_INPUT_LIMIT {
        return Err(Failure::InputSizeLimit);
    }
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|_| Failure::DocxInvalid)?;
    if archive.len() > ENTRY_LIMIT {
        return Err(Failure::DocxExpansionLimit);
    }
    let mut names = HashSet::new();
    let mut total = 0_u64;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(|_| Failure::DocxInvalid)?;
        let name = entry.name().to_owned();
        if name.is_empty()
            || name.starts_with('/')
            || name.contains(['\\', '\0'])
            || name.split('/').any(|part| part == "..")
            || !names.insert(name.clone())
            || entry
                .unix_mode()
                .is_some_and(|mode| mode & 0o170000 == 0o120000)
        {
            return Err(Failure::DocxInvalid);
        }
        total = total
            .checked_add(entry.size())
            .filter(|total| *total <= EXPANDED_LIMIT)
            .ok_or(Failure::DocxExpansionLimit)?;
        if entry.is_dir() {
            continue;
        }
        let mut expanded = Vec::new();
        entry
            .by_ref()
            .take(EXPANDED_LIMIT + 1)
            .read_to_end(&mut expanded)
            .map_err(|_| Failure::DocxInvalid)?;
        if expanded.len() as u64 != entry.size() || expanded.len() as u64 > EXPANDED_LIMIT {
            return Err(Failure::DocxExpansionLimit);
        }
        if name.ends_with(".xml") || name.ends_with(".rels") {
            parse_xml(&expanded)?;
        }
    }
    let target = main_part(&mut archive)?;
    let xml = part(&mut archive, &target)?;
    let document = parse_xml(&xml)?;
    let root = document.root_element();
    if !is_word(root, "document") {
        return Err(Failure::DocxInvalid);
    }
    let body = child(root, "body").ok_or(Failure::DocxInvalid)?;
    let mut reader = WordReader {
        deleted_paragraph: Vec::new(),
    };
    let parsed = reader.many(body.children())?;
    let mut output = String::new();
    let mut units = 0;
    write_raw(&parsed.nodes, &mut output, &mut units)?;
    Ok(output)
}

fn part(archive: &mut zip::ZipArchive<Cursor<&[u8]>>, name: &str) -> Result<Vec<u8>, Failure> {
    let entry = archive.by_name(name).map_err(|_| Failure::DocxInvalid)?;
    let mut bytes = Vec::new();
    entry
        .take(EXPANDED_LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| Failure::DocxInvalid)?;
    if bytes.len() as u64 > EXPANDED_LIMIT {
        return Err(Failure::DocxExpansionLimit);
    }
    Ok(bytes)
}
fn main_part(archive: &mut zip::ZipArchive<Cursor<&[u8]>>) -> Result<String, Failure> {
    let Ok(bytes) = part(archive, "_rels/.rels") else {
        return Ok("word/document.xml".into());
    };
    let xml = parse_xml(&bytes)?;
    let mut targets=xml.root_element().children().filter(|node|node.is_element()&&node.tag_name().name()=="Relationship"&&node.attribute("Type").is_some_and(|kind|kind=="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument"||kind=="http://purl.oclc.org/ooxml/officeDocument/relationships/officeDocument"));
    let Some(target) = targets.next() else {
        return Ok("word/document.xml".into());
    };
    if targets.next().is_some() || target.attribute("TargetMode") == Some("External") {
        return Err(Failure::DocxInvalid);
    }
    let name = target
        .attribute("Target")
        .ok_or(Failure::DocxInvalid)?
        .trim_start_matches('/');
    if name.is_empty()
        || name.contains([':', '\\', '\0'])
        || name.split('/').any(|part| part == ".." || part.is_empty())
    {
        return Err(Failure::DocxInvalid);
    }
    Ok(name.to_owned())
}
fn parse_xml(bytes: &[u8]) -> Result<Document<'_>, Failure> {
    let text = std::str::from_utf8(bytes).map_err(|_| Failure::DocxInvalid)?;
    let document = Document::parse_with_options(
        text,
        ParsingOptions {
            allow_dtd: false,
            nodes_limit: 1_000_000,
            ..Default::default()
        },
    )
    .map_err(|_| Failure::DocxInvalid)?;
    if document
        .descendants()
        .any(|node| node.ancestors().take(66).count() > 65)
    {
        return Err(Failure::DocxExpansionLimit);
    }
    Ok(document)
}
fn is_word(node: Node<'_, '_>, name: &str) -> bool {
    node.is_element()
        && node.tag_name().name() == name
        && matches!(node.tag_name().namespace(), Some(W | W_STRICT))
}
fn child<'a, 'input>(node: Node<'a, 'input>, name: &str) -> Option<Node<'a, 'input>> {
    node.children().find(|node| is_word(*node, name))
}
fn attribute<'a, 'input>(node: Node<'a, 'input>, name: &str) -> Option<&'a str> {
    node.attribute((W, name))
        .or_else(|| node.attribute((W_STRICT, name)))
}

enum Plain {
    Text(String),
    Tab,
    Paragraph(Vec<Plain>),
    Table(Vec<Plain>),
    Row(Vec<Plain>),
    Cell {
        children: Vec<Plain>,
        span: usize,
        continuation: bool,
    },
}
#[derive(Default)]
struct ReadResult {
    nodes: Vec<Plain>,
    extra: Vec<Plain>,
}
struct WordReader<'a, 'input> {
    deleted_paragraph: Vec<Node<'a, 'input>>,
}
impl<'a, 'input> WordReader<'a, 'input> {
    fn many(
        &mut self,
        nodes: impl IntoIterator<Item = Node<'a, 'input>>,
    ) -> Result<ReadResult, Failure> {
        let mut result = ReadResult::default();
        for node in nodes {
            let next = self.read(node)?;
            result.nodes.extend(next.nodes);
            result.extra.extend(next.extra);
        }
        Ok(result)
    }
    fn read(&mut self, node: Node<'a, 'input>) -> Result<ReadResult, Failure> {
        if !node.is_element() {
            return Ok(ReadResult::default());
        }
        let name = node.tag_name().name();
        let namespace = node.tag_name().namespace();
        if namespace == Some(MC) && name == "AlternateContent" {
            return node
                .children()
                .find(|node| {
                    node.tag_name().namespace() == Some(MC) && node.tag_name().name() == "Fallback"
                })
                .map(|node| self.many(node.children()))
                .unwrap_or_else(|| Ok(ReadResult::default()));
        }
        if namespace == Some(VML)
            && ["roundrect", "shape", "textbox", "group", "rect"].contains(&name)
        {
            return self.many(node.children());
        }
        if !matches!(namespace, Some(W | W_STRICT)) {
            return Ok(ReadResult::default());
        }
        match name {
            "p" => {
                if child(node, "pPr")
                    .and_then(|node| child(node, "rPr"))
                    .and_then(|node| child(node, "del"))
                    .is_some()
                {
                    self.deleted_paragraph.extend(node.children());
                    return Ok(ReadResult::default());
                }
                let pending = std::mem::take(&mut self.deleted_paragraph);
                let next = self.many(pending.into_iter().chain(node.children()))?;
                let mut nodes = vec![Plain::Paragraph(next.nodes)];
                nodes.extend(next.extra);
                Ok(ReadResult {
                    nodes,
                    extra: Vec::new(),
                })
            }
            "t" => Ok(value(Plain::Text(
                node.children().filter_map(|node| node.text()).collect(),
            ))),
            "tab" => Ok(value(Plain::Tab)),
            "noBreakHyphen" => Ok(value(Plain::Text("\u{2011}".into()))),
            "softHyphen" => Ok(value(Plain::Text("\u{ad}".into()))),
            "r" | "hyperlink" | "customXml" | "ins" | "moveTo" | "moveFromRangeEnd"
            | "moveFromRangeStart" | "moveToRangeEnd" | "moveToRangeStart" | "object"
            | "smartTag" | "drawing" | "txbxContent" => self.many(node.children()),
            "pict" => {
                let next = self.many(node.children())?;
                let mut extra = next.extra;
                extra.extend(next.nodes);
                Ok(ReadResult {
                    nodes: Vec::new(),
                    extra,
                })
            }
            "tbl" => {
                let mut next = self.many(node.children())?;
                merge_cells(&mut next.nodes);
                Ok(ReadResult {
                    nodes: vec![Plain::Table(next.nodes)],
                    extra: next.extra,
                })
            }
            "tr" => {
                if child(node, "trPr")
                    .and_then(|node| child(node, "del"))
                    .is_some()
                {
                    return Ok(ReadResult::default());
                }
                let next = self.many(node.children())?;
                Ok(ReadResult {
                    nodes: vec![Plain::Row(next.nodes)],
                    extra: next.extra,
                })
            }
            "tc" => {
                let properties = child(node, "tcPr");
                let span = properties
                    .and_then(|node| child(node, "gridSpan"))
                    .and_then(|node| attribute(node, "val"))
                    .map(|span| span.parse::<usize>().map_err(|_| Failure::DocxInvalid))
                    .transpose()?
                    .unwrap_or(1);
                if span == 0 || span > 4096 {
                    return Err(Failure::DocxInvalid);
                }
                let continuation = properties
                    .and_then(|node| child(node, "vMerge"))
                    .is_some_and(|node| {
                        attribute(node, "val").is_none_or(|value| value == "continue")
                    });
                let next = self.many(node.children())?;
                Ok(ReadResult {
                    nodes: vec![Plain::Cell {
                        children: next.nodes,
                        span,
                        continuation,
                    }],
                    extra: next.extra,
                })
            }
            "sdt" => {
                let Some(content) = child(node, "sdtContent") else {
                    return Ok(ReadResult::default());
                };
                let mut next = self.many(content.children())?;
                let checkbox = child(node, "sdtPr").is_some_and(|properties| {
                    properties.children().any(|node| {
                        node.tag_name().namespace() == Some(WORD2010)
                            && node.tag_name().name() == "checkbox"
                    })
                });
                if checkbox && !remove_first_text(&mut next.nodes) {
                    next.nodes.clear();
                }
                Ok(next)
            }
            "sym" => {
                let (Some(font), Some(code)) = (attribute(node, "font"), attribute(node, "char"))
                else {
                    return Ok(ReadResult::default());
                };
                let mapped = u32::from_str_radix(code.trim_start_matches("0x"), 16)
                    .ok()
                    .and_then(|code| dingbats::lookup(font, code))
                    .or_else(|| {
                        (code.len() == 4 && code.starts_with("F0"))
                            .then(|| {
                                u32::from_str_radix(&code[2..], 16)
                                    .ok()
                                    .and_then(|code| dingbats::lookup(font, code))
                            })
                            .flatten()
                    });
                Ok(mapped
                    .map(|character| value(Plain::Text(character.to_string())))
                    .unwrap_or_default())
            }
            _ => Ok(ReadResult::default()),
        }
    }
}
fn value(node: Plain) -> ReadResult {
    ReadResult {
        nodes: vec![node],
        extra: Vec::new(),
    }
}
fn merge_cells(rows: &mut [Plain]) {
    if rows.iter().any(|row|!matches!(row,Plain::Row(cells) if cells.iter().all(|cell|matches!(cell,Plain::Cell{..})))){return;}
    let mut columns = HashSet::new();
    for row in rows {
        let Plain::Row(cells) = row else {
            continue;
        };
        let mut column = 0;
        cells.retain(|cell| {
            let Plain::Cell {
                span, continuation, ..
            } = cell
            else {
                return true;
            };
            let keep = !*continuation || !columns.contains(&column);
            if keep {
                columns.insert(column);
            }
            column += span;
            keep
        });
    }
}
fn remove_first_text(nodes: &mut [Plain]) -> bool {
    for node in nodes {
        match node {
            Plain::Text(text) if !text.is_empty() => {
                text.clear();
                return true;
            }
            Plain::Paragraph(children)
            | Plain::Table(children)
            | Plain::Row(children)
            | Plain::Cell { children, .. } => {
                if remove_first_text(children) {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}
fn write_raw(nodes: &[Plain], output: &mut String, units: &mut usize) -> Result<(), Failure> {
    for node in nodes {
        match node {
            Plain::Text(text) => push(text, output, units)?,
            Plain::Tab => push("\t", output, units)?,
            Plain::Paragraph(children) => {
                write_raw(children, output, units)?;
                push("\n\n", output, units)?;
            }
            Plain::Table(children) | Plain::Row(children) | Plain::Cell { children, .. } => {
                write_raw(children, output, units)?
            }
        }
    }
    Ok(())
}
fn push(text: &str, output: &mut String, units: &mut usize) -> Result<(), Failure> {
    *units = units
        .checked_add(text.encode_utf16().count())
        .filter(|count| *count <= EXTRACTION_TEXT_LIMIT)
        .ok_or(Failure::DocxTextLimit)?;
    output.push_str(text);
    Ok(())
}
