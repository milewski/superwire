```wire
agent greeting {
    model: ollama("qwen3.5:8b")
    inference: {        temperature: 0.7}

    // Leading indentation in this multiline string is neutralized.
    instruction: """
        You are a friendly assistant.
        Write a short welcome message.
        Keep it to one sentence.
    """

    output: string
}
```
---
```wire
agent greeting {
    model: ollama("qwen3.5:8b")
    inference: { temperature: 0.7 }

    // Leading indentation in this multiline string is neutralized.
    instruction: """
        You are a friendly assistant.
        Write a short welcome message.
        Keep it to one sentence.
    """

    output: string
}
```
