pub mod bakeoff;
pub mod corpus;
mod embeddings;
pub mod evaluate;
pub mod evaluation;
mod fts5;
mod hybrid;
pub mod search;
mod sha256;
mod sqlite_cache;
pub mod types;

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
