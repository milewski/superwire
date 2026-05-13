```wire
provider openai from openai {}
model openai_model from openai {id:"gpt-4o-mini"}

agent planner {model: model.openai_model instruction:tool.compose(data:{topic:"engine-ai" tags:["dsl","fmt"]}, meta:{priority:1}) output{value:string}}

output { plan:agent.planner.value }
```
---
```wire
provider openai from openai {
}

model openai_model from openai {
    id: "gpt-4o-mini"
}

agent planner {
    model: model.openai_model

    instruction: tool.compose(
        data: {
            topic: "engine-ai"
            tags: ["dsl", "fmt"]
        },
        meta: { priority: 1 },
    )

    output {
        value: string
    }
}

output {
    plan: agent.planner.value
}
```
