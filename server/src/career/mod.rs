//! Pure career score, activity, artifact, and legacy-bridge rules (§2.3–§4).

mod activity;
mod artifact;
mod recruitment;
mod score;
mod types;

pub use activity::create_activity_planner;
pub use artifact::create_artifact_rules;
pub use recruitment::*;
pub use score::{create_bridge_evidence_planner, create_spec_score_rules};
pub use types::*;
