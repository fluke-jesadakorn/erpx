//! Native Quality owner Work projection and injectable source gate.
pub use crate::adapters::quality_work::{FnSourceAccess, MappedSourceAccess};
pub use crate::application::quality::{
    QualitySourceAccess, QualityWork, QualityWorkItem, QualityWorkSource, SourceAccess,
    quality_work, quality_work_from_workspace, quality_work_with_same_person,
};
