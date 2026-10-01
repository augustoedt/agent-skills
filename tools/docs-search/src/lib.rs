#[cfg(feature = "experimental-adapters")]
pub mod bakeoff;
pub mod corpus;
#[cfg(feature = "experimental-adapters")]
mod embeddings;
pub mod evaluate;
pub mod evaluation;
#[cfg(feature = "experimental-adapters")]
mod fts5;
#[cfg(feature = "experimental-adapters")]
mod hybrid;
pub mod search;
#[cfg(feature = "experimental-adapters")]
mod sha256;
#[cfg(feature = "experimental-adapters")]
mod sqlite_cache;
pub mod types;

#[cfg(feature = "experimental-adapters")]
pub use bakeoff::{
    FinalizeRequest, HoldoutDecideRequest, HoldoutFinalizeRequest, HoldoutObserveRequest,
    ObserveRequest, finalize, holdout_decide, holdout_finalize, holdout_observe, observe,
};
pub use evaluate::{EVALUATION_REPORT_SCHEMA_VERSION, EvaluateRequest, EvaluationReport, evaluate};
pub use evaluation::{
    EVALUATION_SCHEMA_VERSION, EvaluationCategory, EvaluationQuery, EvaluationSet,
    parse_evaluation_set, validate_evaluation_set,
};
pub use search::search;
pub use types::{SCHEMA_VERSION, SearchRequest, SearchResponse};
