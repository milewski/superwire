mod api {
    pub use superwire_executor::api::*;
}

mod model {
    pub use superwire_executor::model::*;
}

mod runtime {
    pub use superwire_executor::runtime::*;
}

mod service {
    pub use superwire_executor::service::*;
}

#[macro_use]
mod support;

use serde_json::json;
use support::fixtures;
use support::runner::{Format, TestRunner};

#[tokio::test]
async fn asserts_json_schema_response_format() {
    let run_output = TestRunner::workflow(fixtures::AGENT_RESPONSE_FORMAT_JSON_SCHEMA)
        .provider("openai", |provider| {
            provider.api_key("test-api-key").model("model-a", |model| {
                model.turn().with_response_format(Format::JsonSchema).respond_string("hello");
            });
        })
        .run()
        .await
        .expect("fixture runner should assert json_schema response format");

    assert_eq!(run_output.output, json!({ "greeting": "hello" }));
}
