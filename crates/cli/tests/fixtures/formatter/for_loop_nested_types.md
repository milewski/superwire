```ai
provider openai {driver:"openai" models:[ "gpt-4o-mini"]}
input { items:[string] }

agent reviewer for item 
in input.items {model:openai("gpt-4o-mini") prompt:"Review {{item}}" output:{score:number tags:[  
string]}}

output { reviews:agent.reviewer }
```
---
```ai
provider openai {
    driver: "openai"
    models: [
        "gpt-4o-mini",
    ]
}

input {
    items: [string]
}

agent reviewer for item in input.items {
    model: openai("gpt-4o-mini")
    prompt: "Review {{ item }}"
    output: {
        score: number
        tags: [string]
    }
}

output {
    reviews: agent.reviewer
}
```
