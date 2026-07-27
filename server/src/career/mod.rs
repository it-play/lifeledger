//! Pure career rules for progression, recruitment, payroll, employment tax, and military (§2.3–§12).

mod activity;
mod artifact;
mod employment_tax;
mod military;
mod payroll;
mod recruitment;
mod score;
mod types;

pub use activity::create_activity_planner;
pub use artifact::create_artifact_rules;
pub use employment_tax::create_employment_tax_rules;
pub use military::create_military_rules;
pub use payroll::create_payroll_rules;
pub use recruitment::*;
pub use score::{create_bridge_evidence_planner, create_spec_score_rules};
pub use types::*;
