```wire
provider openai from openai {}
model openai_model from openai {id:"gpt-4o-mini"}

agent planner {model: model.openai_model context:{project:"engine-ai" details:{owner:"core" active:true} ids:[1,2,3,]} instruction:"Plan" output:string}

output { plan:agent.planner }
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

    context: {
        project: "engine-ai"
        details: {
            owner: "core"
            active: true
        }
        ids: [1, 2, 3]
    }

    instruction: "Plan"
    output: string
}

output {
    plan: agent.planner
}
```
