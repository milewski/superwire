use engine_ai_core::workflow;

#[test]
fn test_terminal_agent_with_output_block() {
    let workflow = workflow! {
        provider ollama1 {
            driver <- "ollama"
            models <- ["qwen3:8b"]
        }

        <- agent summary {
            model <- "ollama1/qwen3:8b"
            output <- {
                text: string
            }
            prompt <- "Summarize AI"
        }

        <- agent keywords {
            model <- "ollama1/qwen3:8b"
            output <- {
                words: [string]
            }
            prompt <- "Extract keywords"
        }

        output {
            topic <- "artificial intelligence"
            timestamp <- "2026-03-08T10:00:00Z"
        }
    };

    assert_eq!(workflow.agents.len(), 2);
    assert!(workflow.agents[0].is_terminal);
    assert!(workflow.agents[1].is_terminal);

    assert!(workflow.output.is_some());
    let output_block = workflow.output.unwrap();
    assert_eq!(output_block.fields.len(), 2);
}

#[test]
fn test_single_terminal_agent() {
    let workflow = workflow! {
        provider ollama1 {
            driver <- "ollama"
            models <- ["qwen3:8b"]
        }

        <- agent list {
            model <- "ollama1/qwen3:8b"
            prompt <- "create a list from 0 to 10"
        }
    };

    assert_eq!(workflow.agents.len(), 1);
    assert!(workflow.agents[0].is_terminal);
    assert!(workflow.output.is_none());
}

#[test]
fn test_multiple_terminal_agents() {
    let workflow = workflow! {
        provider ollama1 {
            driver <- "ollama"
            models <- ["qwen3:8b"]
        }

        <- agent list {
            model <- "ollama1/qwen3:8b"
            prompt <- "create a list from 0 to 10"
        }

        <- agent atoz {
            model <- "ollama1/qwen3:8b"
            prompt <- "spell out all letters from alphabet from a to z"
        }
    };

    assert_eq!(workflow.agents.len(), 2);
    assert!(workflow.agents[0].is_terminal);
    assert!(workflow.agents[1].is_terminal);
}

#[test]
fn test_output_block_only() {
    let workflow = workflow! {
        provider ollama1 {
            driver <- "ollama"
            models <- ["qwen3:8b"]
        }

        agent research {
            model <- "ollama1/qwen3:8b"
            output <- {
                summary: string
            }
            prompt <- "Research AI"
        }

        output {
            research_summary <- agent.research.summary
        }
    };

    assert_eq!(workflow.agents.len(), 1);
    assert!(!workflow.agents[0].is_terminal);

    assert!(workflow.output.is_some());
    let output_block = workflow.output.unwrap();
    assert_eq!(output_block.fields.len(), 1);
}
