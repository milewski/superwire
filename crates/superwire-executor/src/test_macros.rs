macro_rules! execute {
    ($fixture:expr $(,)?) => {
        async move {
            let workflow_source = superwire_core::testing::WorkflowSource::inline($fixture)
                .read()
                .expect("inline workflow source should read");

            $crate::tests::support::execute(&workflow_source, vec![]).await
        }
    };

    ($fixture:expr, input: $input:tt $(,)?) => {
        async move {
            let workflow_source = superwire_core::testing::WorkflowSource::inline($fixture)
                .read()
                .expect("inline workflow source should read");

            $crate::tests::support::execute_with_input(&workflow_source, vec![], serde_json::json!($input)).await
        }
    };

    ($fixture:expr, input: $input:tt, $(output: $output:tt),+ $(,)?) => {
        async move {
            let workflow_source = superwire_core::testing::WorkflowSource::inline($fixture)
                .read()
                .expect("inline workflow source should read");

            $crate::tests::support::execute_with_input(
                &workflow_source,
                vec![$(serde_json::json!($output)),+],
                serde_json::json!($input),
            )
            .await
        }
    };

    ($fixture:expr, $(output: $output:tt),+ $(,)?) => {
        async move {
            let workflow_source = superwire_core::testing::WorkflowSource::inline($fixture)
                .read()
                .expect("inline workflow source should read");

            $crate::tests::support::execute(&workflow_source, vec![$(serde_json::json!($output)),+]).await
        }
    };
}

macro_rules! execute_error {
    ($fixture:expr $(,)?) => {
        async move {
            let workflow_source = superwire_core::testing::WorkflowSource::inline($fixture)
                .read()
                .expect("inline workflow source should read");

            $crate::tests::support::execute_expect_error(&workflow_source, vec![]).await
        }
    };

    ($fixture:expr, input: $input:tt $(,)?) => {
        async move {
            let workflow_source = superwire_core::testing::WorkflowSource::inline($fixture)
                .read()
                .expect("inline workflow source should read");

            $crate::tests::support::execute_with_input_expect_error(&workflow_source, vec![], serde_json::json!($input)).await
        }
    };
}

macro_rules! execute_secrets {
    ($fixture:expr, input: $input:tt, secrets: $secrets:tt, $(output: $output:tt),+ $(,)?) => {
        async move {
            let workflow_source = superwire_core::testing::WorkflowSource::inline($fixture)
                .read()
                .expect("inline workflow source should read");

            $crate::tests::support::execute_with_secrets(
                &workflow_source,
                vec![$(serde_json::json!($output)),+],
                serde_json::json!($input),
                serde_json::json!($secrets),
            )
            .await
        }
    };
}

macro_rules! execute_secrets_error {
    ($fixture:expr, input: $input:tt, secrets: $secrets:tt $(,)?) => {
        async move {
            let workflow_source = superwire_core::testing::WorkflowSource::inline($fixture)
                .read()
                .expect("inline workflow source should read");

            $crate::tests::support::execute_with_secrets_expect_error(
                &workflow_source,
                vec![],
                serde_json::json!($input),
                serde_json::json!($secrets),
            )
            .await
        }
    };
}
