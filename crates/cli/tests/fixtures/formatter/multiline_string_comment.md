```wire
agent greeting {
    model: model.ollama_model
    // Leading indentation in this multiline string is neutralized.
    instruction: """
        You are a friendly assistant.
        Write a short welcome message.
        Keep it to one sentence.
    """
    
    output {
        value: string
    }
}
```
---
```wire
agent greeting {
    model: model.ollama_model

    // Leading indentation in this multiline string is neutralized.
    instruction: """
        You are a friendly assistant.
        Write a short welcome message.
        Keep it to one sentence.
    """

    output {
        value: string
    }
}
```
