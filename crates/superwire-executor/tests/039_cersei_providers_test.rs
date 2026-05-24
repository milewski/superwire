#[macro_use]
mod support;

use superwire_dsl::{parse_workflow, validate_workflow};
use support::fixtures;

#[test]
fn accepts_all_cersei_provider_drivers() {
    let workflow = parse_workflow(fixtures::CERSEI_PROVIDERS).expect("provider fixture should parse");
    let validation_report = validate_workflow(&workflow);

    assert!(!validation_report.has_issues(), "provider fixture should validate");
}
