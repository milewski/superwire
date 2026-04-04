```ai
agent greeting {
    model: ollama("qwen3.5:8b")
    // Leading indentation in this multiline string is neutralized.
    prompt: """
        You are a friendly assistant.
        Write a short welcome message.
        Keep it to one sentence.
    """
    
    output: string
}
```
---
```ai
agent greeting {
    model: ollama("qwen3.5:8b")

    // Leading indentation in this multiline string is neutralized.
    prompt: """
        You are a friendly assistant.
        Write a short welcome message.
        Keep it to one sentence.
    """

    output: string
}
```
