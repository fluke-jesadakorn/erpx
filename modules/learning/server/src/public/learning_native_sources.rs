//! Native Learning ownership-source contract. Callers substitute only the
//! frozen union and one owner-controlled alias.
pub use crate::adapters::learning_native_sources::{
    LEARNING_NATIVE_SOURCES_JSON, LearningNativeSources, learning_native_source_union_sql,
    learning_native_sources, render_learning_native_source_probe,
};
