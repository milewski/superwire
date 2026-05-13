```wire
provider openai from openai{}
model openai_model from openai{id:"gpt-4o-mini"}

agent number_note for n in [1,2,3,4] {model: model.openai_model instruction:"Number {{           n}}" output:{number:number note:string}}

output { notes:agent . number_note }
```
---
```wire
provider openai from openai {
}

model openai_model from openai {
    id: "gpt-4o-mini"
}

agent number_note for n in [1, 2, 3, 4] {
    model: model.openai_model
    instruction: "Number {{ n }}"
    output: {
        number: number
        note: string
    }
}

output {
    notes: agent.number_note
}
```
