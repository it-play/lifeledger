//! Pure life-simulation rules for living costs and later M4 slices.

mod corporation;
mod credit;
mod insolvency;
mod insurance;
mod lease;
mod life_event;
mod living_cost;
mod loan;
mod property;
mod property_tax;
mod real_estate;
mod types;
mod welfare;

pub use corporation::create_corporation_rules;
pub use credit::create_credit_rules;
pub use insolvency::{create_insolvency_rules, create_insolvency_rules_with_loan_rules};
pub use insurance::{
    create_fictional_family_care_insurance_catalog, create_insurance_rules,
    create_insurance_rules_with_hasher,
};
pub use lease::create_lease_rules;
pub use life_event::{create_life_event_rules, create_life_event_rules_with_entropy};
pub use living_cost::create_living_cost_rules;
pub use loan::create_loan_rules;
pub use property::create_property_rules;
pub use property_tax::create_property_tax_rules;
pub use real_estate::create_real_estate_rules;
pub use types::*;
pub use welfare::{create_fictional_restart_grant_program, create_welfare_rules};
