// Workflow integration tests
// These tests validate the complete DSL functionality against real workflow files
// Tests use cached LLM responses stored in tests/.cache/ for fast execution
#[macro_use]
mod helpers;

#[cfg(test)]
mod test {
    use crate::{input, workflow};
    use serde::Deserialize;
    use serde_json::Value;

    #[tokio::test]
    async fn test_basic_workflow() {
        #[derive(Deserialize)]
        struct Output {
            greeting: String,
        }

        let output = workflow!("workflows/basic.ai" => Output).await;

        assert!(output.greeting.contains("AI assistant"));
    }

    #[tokio::test]
    async fn test_input_output_workflow() {
        let inputs = input!(topic: "Rust", audience: "developers");

        #[derive(Deserialize)]
        struct Output {
            topic: String,
            audience: String,
            summary: String,
        }

        let output = workflow!(inputs => "workflows/input_output.ai" => Output).await;

        assert_eq!(output.topic, "Rust");
        assert_eq!(output.audience, "developers");
        assert!(output.summary.contains("Rust"));
    }

    #[tokio::test]
    async fn test_schema_workflow() {
        #[derive(Deserialize)]
        struct Output {
            person: Person,
        }

        #[derive(Deserialize)]
        struct Person {
            name: String,
            age: u32,
            hobbies: Vec<String>,
        }

        let output = workflow!("workflows/schema.ai" => Output).await;

        assert!(!output.person.name.is_empty());
        assert!(output.person.age > 0);
        assert!(!output.person.hobbies.is_empty());
    }

    #[tokio::test]
    async fn test_inline_schema_workflow() {
        #[derive(Deserialize)]
        struct Output {
            person: Person,
        }

        #[derive(Deserialize)]
        struct Person {
            name: String,
            age: u32,
            city: String,
        }

        let output = workflow!("workflows/inline_schema.ai" => Output).await;

        assert!(!output.person.name.is_empty());
        assert!(output.person.age > 0);
        assert!(!output.person.city.is_empty());
    }

    #[tokio::test]
    async fn test_parallel_execution_workflow() {
        #[derive(Deserialize)]
        struct Output {
            joke: String,
            fact: String,
            quote: String,
        }

        let output = workflow!("workflows/parallel_execution.ai" => Output).await;

        assert!(!output.joke.is_empty());
        assert!(!output.fact.is_empty());
        assert!(!output.quote.is_empty());
    }

    #[tokio::test]
    async fn test_enum_schema_workflow() {
        #[derive(Deserialize)]
        struct Output {
            weather: Weather,
        }

        #[derive(Deserialize)]
        struct Weather {
            condition: String,
            temperature: f64,
        }

        let output = workflow!("workflows/enum_schema.ai" => Output).await;

        assert!(["sunny", "rainy", "cloudy", "snowy"].contains(&output.weather.condition.as_str()));
        assert!(output.weather.temperature >= -50.0 && output.weather.temperature <= 50.0);
    }

    #[tokio::test]
    async fn test_dependencies_workflow() {
        #[derive(Deserialize)]
        struct Output {
            article: String,
        }

        let output = workflow!("workflows/dependencies.ai" => Output).await;

        assert!(!output.article.is_empty());
    }

    #[tokio::test]
    async fn test_context_sharing_workflow() {
        #[derive(Deserialize)]
        struct Output {
            conversation_continue: String,
        }

        let output = workflow!("workflows/context_sharing.ai" => Output).await;

        assert!(!output.conversation_continue.is_empty());
    }

    #[tokio::test]
    async fn test_for_each_workflow() {
        #[derive(Deserialize)]
        struct Output {
            doubled: Vec<f64>,
            numbers: Vec<f64>,
        }

        let output = workflow!("workflows/for_each.ai" => Output).await;

        assert_eq!(output.doubled.len(), 3);
        assert_eq!(output.numbers.len(), 3);

        // Verify all values are positive numbers
        for value in &output.doubled {
            assert!(*value > 0.0);
        }
        for value in &output.numbers {
            assert!(*value > 0.0);
        }
    }

    #[tokio::test]
    async fn test_string_interpolation_workflow() {
        #[derive(Deserialize)]
        struct Output {
            story: String,
        }

        let output = workflow!("workflows/string_interpolation.ai" => Output).await;

        assert!(!output.story.is_empty());
    }

    #[tokio::test]
    async fn test_schema_descriptions_workflow() {
        #[derive(Deserialize)]
        struct Output {
            user: User,
        }

        #[derive(Deserialize)]
        struct User {
            username: String,
            email: String,
            age: u32,
        }

        let output = workflow!("workflows/schema_descriptions.ai" => Output).await;

        assert!(output.user.username.len() >= 3 && output.user.username.len() <= 20);
        assert!(output.user.email.contains('@'));
        assert!(output.user.age >= 13 && output.user.age <= 120);
    }

    #[tokio::test]
    async fn test_multiline_prompt_workflow() {
        #[derive(Deserialize)]
        struct Output {
            story: String,
        }

        let output = workflow!("workflows/multiline_prompt.ai" => Output).await;

        assert!(!output.story.is_empty());
        assert!(output.story.len() < 1000);
    }

    #[tokio::test]
    async fn test_nullable_schema_workflow() {
        #[derive(Deserialize)]
        struct Output {
            person: Person,
        }

        #[derive(Deserialize)]
        struct Person {
            name: String,
            age: u32,
            #[allow(dead_code)]
            nickname: Option<String>,
            #[allow(dead_code)]
            email: Option<String>,
        }

        let output = workflow!("workflows/nullable_schema.ai" => Output).await;

        assert!(!output.person.name.is_empty());
        assert!(output.person.age > 0);
    }

    #[tokio::test]
    async fn test_compact_syntax_workflow() {
        #[derive(Deserialize)]
        struct Output {
            single_context_summary: Vec<Value>,
            multi_context_summary: Vec<Value>,
        }

        let output = workflow!("workflows/compact_syntax_test.ai" => Output).await;

        assert!(!output.single_context_summary.is_empty());
        assert!(!output.multi_context_summary.is_empty());
    }

    #[tokio::test]
    async fn test_auto_unwrap_workflow() {
        #[derive(Deserialize)]
        struct Output {
            single_unwrapped: String,
            single_explicit: String,
            multi_full: MultiField,
            multi_name: String,
            multi_age: u32,
        }

        #[derive(Deserialize)]
        struct MultiField {
            name: String,
            age: u32,
        }

        let output = workflow!("workflows/auto_unwrap_test.ai" => Output).await;

        assert!(!output.single_unwrapped.is_empty());
        assert!(!output.single_explicit.is_empty());
        assert_eq!(output.single_unwrapped, output.single_explicit);
        assert!(!output.multi_full.name.is_empty());
        assert_eq!(output.multi_full.name, output.multi_name);
        assert_eq!(output.multi_full.age, output.multi_age);
    }

    #[tokio::test]
    async fn test_agent_loop_workflow() {
        #[derive(Deserialize)]
        struct Output {
            result: String,
        }

        let output = workflow!("workflows/agent_loop_test.ai" => Output).await;

        assert!(!output.result.is_empty());
    }

    #[tokio::test]
    async fn test_no_schema_done_workflow() {
        #[derive(Deserialize)]
        struct Output {
            simple: String,
        }

        let output = workflow!("workflows/no_schema_done.ai" => Output).await;

        assert!(!output.simple.is_empty());
    }

    #[tokio::test]
    async fn test_terminal_with_output_workflow() {
        let inputs = input!(user_name: "Alice");

        #[derive(Deserialize)]
        struct Output {
            user: String,
            timestamp: String,
            greeting: String,
        }

        let output = workflow!(inputs => "workflows/terminal_with_output.ai" => Output).await;

        assert_eq!(output.user, "Alice");
        assert!(!output.timestamp.is_empty());
        assert!(!output.greeting.is_empty());
    }

    #[tokio::test]
    async fn test_multiple_terminal_workflow() {
        #[derive(Deserialize)]
        struct Output {
            joke: String,
            fact: String,
        }

        let output = workflow!("workflows/multiple_terminal.ai" => Output).await;

        assert!(!output.joke.is_empty());
        assert!(!output.fact.is_empty());
    }

    #[tokio::test]
    async fn test_compact_context_workflow() {
        let inputs = input!(topic: "artificial intelligence");

        #[derive(Deserialize)]
        struct Output {
            topic: String,
            summary: Vec<Value>,
            research_context: Vec<Value>,
        }

        let output = workflow!(inputs => "workflows/compact_context.ai" => Output).await;

        assert_eq!(output.topic, "artificial intelligence");
        assert!(!output.summary.is_empty());
        assert!(!output.research_context.is_empty());
    }

    #[tokio::test]
    async fn test_simple_inline_type_workflow() {
        #[derive(Deserialize)]
        struct Output {
            sum: u32,
            greeting: String,
            is_hundred: bool,
        }

        let output = workflow!("workflows/simple_inline_type.ai" => Output).await;

        assert_eq!(output.sum, 100);
        assert!(!output.greeting.is_empty());
        assert!(output.is_hundred);
    }

    #[tokio::test]
    async fn test_inline_type_demo_workflow() {
        #[derive(Deserialize)]
        struct Output {
            calculation: u32,
            greeting: String,
            is_large: bool,
            languages: Vec<String>,
            summary: String,
        }

        let output = workflow!("workflows/inline_type_demo.ai" => Output).await;

        assert_eq!(output.calculation, 105);
        assert!(!output.greeting.is_empty());
        assert!(output.is_large);
        assert_eq!(output.languages.len(), 5);
        assert!(!output.summary.is_empty());
    }

    #[tokio::test]
    async fn test_for_each_context_workflow() {
        #[derive(Deserialize)]
        struct Output {
            items: Vec<String>,
            descriptions: Vec<Description>,
            descriptions_context: Vec<Value>,
        }

        #[derive(Deserialize)]
        struct Description {
            #[allow(dead_code)]
            description: String,
        }

        let output = workflow!("workflows/for_each_context_test.ai" => Output).await;

        assert_eq!(output.items.len(), 2);
        assert_eq!(output.descriptions.len(), 2);
        assert!(!output.descriptions_context.is_empty());
    }
}
