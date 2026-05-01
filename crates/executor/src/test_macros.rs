macro_rules! execute {
    ($fixture:expr $(,)?) => {
        $crate::tests::support::execute($fixture, vec![])
    };

    ($fixture:expr, input: $input:tt $(,)?) => {
        $crate::tests::support::execute_with_input($fixture, vec![], serde_json::json!($input))
    };

    ($fixture:expr, input: $input:tt, $(output: $output:tt),+ $(,)?) => {
        $crate::tests::support::execute_with_input($fixture, vec![$(serde_json::json!($output)),+], serde_json::json!($input))
    };

    ($fixture:expr, $(output: $output:tt),+ $(,)?) => {
        $crate::tests::support::execute($fixture, vec![$(serde_json::json!($output)),+])
    };
}

macro_rules! execute_error {
    ($fixture:expr $(,)?) => {
        $crate::tests::support::execute_expect_error($fixture, vec![])
    };

    ($fixture:expr, input: $input:tt $(,)?) => {
        $crate::tests::support::execute_with_input_expect_error($fixture, vec![], serde_json::json!($input))
    };
}

macro_rules! execute_secrets {
    ($fixture:expr, input: $input:tt, secrets: $secrets:tt, $(output: $output:tt),+ $(,)?) => {
        $crate::tests::support::execute_with_secrets(
            $fixture,
            vec![$(serde_json::json!($output)),+],
            serde_json::json!($input),
            serde_json::json!($secrets),
        )
    };
}

macro_rules! execute_secrets_error {
    ($fixture:expr, input: $input:tt, secrets: $secrets:tt $(,)?) => {
        $crate::tests::support::execute_with_secrets_expect_error($fixture, vec![], serde_json::json!($input), serde_json::json!($secrets))
    };
}
