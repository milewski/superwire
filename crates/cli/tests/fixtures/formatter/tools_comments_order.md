```wire
agent assistant_with_tools {
    model: openai("gpt-4.1-mini")

    // The agent can decide when to call these tools.
    // Tool arguments can use literals, references, and secrets.
    tools: [
        // No arguments.
        tool.web_search,

        // One argument.
        tool.knowledge_base_search(password: secrets.knowledge_base_password),

        // Multiple arguments.
        tool.issue_tracker_lookup(project: "engine-ai", status: "open", token: secrets.issue_tracker_token)
    ]

    prompt: "Answer the question using tools when needed: {{ input.question }}"
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
    // Tool arguments can use literals, references, and secrets.
    tools: [
        // No arguments.
        tool.web_search,

        // One argument.
        tool.knowledge_base_search(password: secrets.knowledge_base_password),

        // Multiple arguments.
        tool.issue_tracker_lookup(project: "engine-ai", status: "open", token: secrets.issue_tracker_token),
    ]

    prompt: "Answer the question using tools when needed: {{ input.question }}"
    output: {
        answer: string
        sources: [string]
    }
}
```
