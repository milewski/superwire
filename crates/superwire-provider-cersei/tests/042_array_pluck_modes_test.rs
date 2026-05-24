#[macro_use]
mod support;

use serde_json::json;
use support::fixtures;
use support::runner::TestRunner;

#[tokio::test]
async fn plucks_arrays_with_all_non_null_and_strict_modes() {
    let output = TestRunner::workflow(fixtures::ARRAY_PLUCK_MODES)
        .run()
        .await
        .expect("array pluck modes fixture should execute");

    assert_eq!(
        output.output,
        json!({
            "example1_all_flat": [1, null, null, null, "mixed"],
            "example2_non_null_flat": [1, "mixed"],
            "example3_strict_flat": [1, 2, 3],
            "example4_all_nested": [10, null, null, 20, null, null, "thirty"],
            "example5_non_null_nested": [10, 20, "thirty"],
            "example6_strict_nested": ["a", "b", "c"],
            "example7_array_values_all": [[1, null], [2, null], 4, null, null],
            "example8_array_values_non_null": [[1, null], [2, null], 4],
            "example9_array_values_strict": [[1], [2], [3]],
            "example10_array_items_all": [null, null, null, null, null, null],
            "example11_array_items_non_null": [],
        })
    );
}
