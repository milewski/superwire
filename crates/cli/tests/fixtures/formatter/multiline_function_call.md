```wire
provider openai {driver:"openai" models:["gpt-4o-mini"]}

agent planner {model:openai("gpt-4o-mini") instruction:tool.compose(data:{topic:"engine-ai" tags:["dsl","fmt"]}, meta:{priority:1}) output:string}

output { plan:agent.planner }
```
---
```wire
provider openai {
    driver: "openai"
    models: ["gpt-4o-mini"]
}

agent planner {
    model: openai("gpt-4o-mini")

    instruction: tool.compose(
        data: {
            topic: "engine-ai"
            tags: ["dsl", "fmt"]
        },
        meta: { priority: 1 },
    )

    output: string
}

output {
    plan: agent.planner
}
```
