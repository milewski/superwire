/// Macro to easily define tools with automatic name inference
///
/// # Examples
///
/// ```
/// use engine_ai_agent::tool;
/// use serde::{Deserialize, Serialize};
/// use schemars::JsonSchema;
///
/// tool! {
///     /// Search for information on the web
///     SearchTool {
///         query: String,
///         max_results: Option<usize>,
///     } => async |input| {
///         // Tool implementation
///         Ok(serde_json::json!({
///             "results": format!("Found results for: {}", input.query)
///         }))
///     }
/// }
/// ```
#[macro_export]
macro_rules! register_tool {
    ($tool_type:ty) => {
        $crate::inventory::submit! {
            $crate::tool::ToolRegistration::from_default::<$tool_type>()
        }
    };
}

#[macro_export]
macro_rules! tool {
    (
        $(#[$meta:meta])*
        $name:ident {
            $($field:ident: $field_type:ty),* $(,)?
        } => async |$input:ident| $body:block
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Default)]
        pub struct $name;

        paste::paste! {
            #[derive(Debug, Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
            pub struct [<$name Input>] {
                $(pub $field: $field_type),*
            }

            #[async_trait::async_trait]
            impl $crate::tool::Tool for $name {
                type Input = [<$name Input>];

                fn description(&self) -> &'static str {
                    concat!($(stringify!($meta), " "),*)
                }

                async fn execute(&self, $input: Self::Input) -> Result<serde_json::Value, $crate::tool::ToolError> {
                    $body
                }
            }
        }

        $crate::register_tool!($name);
    };
}
