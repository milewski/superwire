```wire
agent assistant_with_tools {
    model: openai("gpt-4.1-mini")

    // The agent can decide when to call these tools.
    // Tool binding overrides can use literals, references, and secrets.
    uses: [
        // No arguments.
        tool.web_search,

        // One binding override.
        tool.knowledge_base_search {
            bindings {
                password: secrets.knowledge_base_password
            }
        },

        // Multiple binding overrides.
        tool.issue_tracker_lookup {
            bindings {
                project: "engine-ai"
                status: "open"
                token: secrets.issue_tracker_token
            }
        }
    ]

    instruction: "Answer the question using tools when needed: {{ input.question }}"
    output: {
        answer: string
        sources: [string]
    }
}
```
---
```wire
agent assistant_with_tools {
    model: openai("gpt-4.1-mini")

    // The agent can decide when to call these tools.
    // Tool binding overrides can use literals, references, and secrets.
    uses: [
        // No arguments.
        tool.web_search,

        // One binding override.
        tool.knowledge_base_search {
            bindings {
                password: secrets.knowledge_base_password
            }
        },

        // Multiple binding overrides.
        tool.issue_tracker_lookup {
            bindings {
                project: "engine-ai"
                status: "open"
                token: secrets.issue_tracker_token
            }
        },
    ]

    instruction: "Answer the question using tools when needed: {{ input.question }}"
    output: {
        answer: string
        sources: [string]
    }
}
```
