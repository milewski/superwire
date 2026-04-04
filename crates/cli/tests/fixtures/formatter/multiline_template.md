```ai
provider openai {driver:"openai" models:["gpt-4o-mini"]}
input { topic:string }

agent writer {model:openai("gpt-4o-mini") prompt:"""Write about {{input.topic}}
Keep it short and clear.""" output:string}

agent writer2 {
    model: openai("gpt-4o-mini")
    prompt: """
    Write about {{ input.topic }}
    Keep it short and clear.
    """
    output: string
}

output { text:agent.writer }
```
---
```ai
provider openai {
    driver: "openai"
    models: ["gpt-4o-mini"]
}

input {
    topic: string
}

agent writer {
    model: openai("gpt-4o-mini")

    prompt: """
        Write about {{ input.topic }}
        Keep it short and clear.
    """

    output: string
}

agent writer2 {
    model: openai("gpt-4o-mini")

    prompt: """
        Write about {{ input.topic }}
        Keep it short and clear.
    """

    output: string
}

output {
    text: agent.writer
}
```
