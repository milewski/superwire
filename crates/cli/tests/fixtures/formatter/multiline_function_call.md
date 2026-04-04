```ai
provider openai {driver:"openai" models:["gpt-4o-mini"]}

agent planner {model:openai("gpt-4o-mini") prompt:tool.compose(data:{topic:"engine-ai" tags:["dsl","fmt"]}, meta:{priority:1}) output:string}

output { plan:agent.planner }
```
---
```ai
provider openai {
    driver: "openai"
    models: ["gpt-4o-mini"]
}

agent planner {
    model: openai("gpt-4o-mini")

    prompt: tool.compose(
        data: {
            topic: "engine-ai"
            tags: ["dsl", "fmt"]
        },
        meta: {
            priority: 1
        },
    )

    output: string
}

output {
    plan: agent.planner
}
```
