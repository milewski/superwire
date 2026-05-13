```wire
provider openai from openai {}
model openai_model from openai {id:"gpt-4o-mini"}
input { items:[string] }
agent reviewer for item 
in input.items {model: model.openai_model instruction:"Review {{item}}" output:{score:number tags:[  
string]}}
output { reviews:agent.reviewer }
```
---
```wire
provider openai from openai {
}

model openai_model from openai {
    id: "gpt-4o-mini"
}

input {
    items: [string]
}

agent reviewer for item in input.items {
    model: model.openai_model
    instruction: "Review {{ item }}"
    output: {
        score: number
        tags: [string]
    }
}

output {
    reviews: agent.reviewer
}
```
