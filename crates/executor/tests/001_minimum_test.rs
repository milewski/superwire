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
use support::runner::TestRunner;

#[tokio::test]
async fn executes_fixture_through_scripted_provider_server() {
    let run_output = TestRunner::workflow(fixtures::MINIMUM)
        .provider("openai", |provider| {
            provider.api_key("test-api-key").model("model-a", |model| {
                model
                    .turn()
                    .with_messages(|messages| {
                        assert_eq!(messages.len(), 2);
                        assert_eq!(messages.last().and_then(|message| message.get("role")), Some(&json!("user")));
                    })
                    .expect_prompt("Write a short welcome message.")
                    .respond_string("hello from fixture runner");
            });
        })
        .run()
        .await
        .expect("fixture runner should execute workflow");

    assert_eq!(run_output.output, json!({ "greeting": "hello from fixture runner" }));
    assert_eq!(run_output.provider_requests["openai"].len(), 1);
}
