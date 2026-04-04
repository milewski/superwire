```ai
provider openai {driver:"openai" models:["gpt-4o-mini"]}

agent planner {model:openai("gpt-4o-mini") context:{project:"engine-ai" details:{owner:"core" active:true} ids:[1,2,3,]} prompt:"Plan" output:string}

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
    context: {
        project: "engine-ai"
        details: {
            owner: "core"
            active: true
        }
        ids: [1, 2, 3]
    }
    prompt: "Plan"
    output: string
}

output {
    plan: agent.planner
}
```
