```wire
provider openai from openai {}
model openai_model from openai {id:"gpt-4o-mini"}
input { topic:string }

agent writer {model: model.openai_model instruction:"""Write about {{input.topic}}
Keep it short and clear.""" output{value:string}}

agent writer2 {
    model: model.openai_model
    instruction: """
    Write about {{ input.topic }}
    Keep it short and clear.
    """
    output {
        value: string
    }
}

output { text:agent.writer.value }
```
---
```wire
provider openai from openai {
}

model openai_model from openai {
    id: "gpt-4o-mini"
}

input {
    topic: string
}

agent writer {
    model: model.openai_model

    instruction: """
        Write about {{ input.topic }}
        Keep it short and clear.
    """

    output {
        value: string
    }
}

agent writer2 {
    model: model.openai_model

    instruction: """
        Write about {{ input.topic }}
        Keep it short and clear.
    """

    output {
        value: string
    }
}

output {
    text: agent.writer.value
}
```
