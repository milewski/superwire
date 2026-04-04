```ai
provider openai{driver:"openai" models:["gpt-4o-mini"]}

agent number_note for n in [1,2,3,4] {model:openai("gpt-4o-mini" ) prompt:"Number {{           n}}" output:{number:number note:string}}

output { notes:agent . number_note }
```
---
```ai
provider openai {
    driver: "openai"
    models: ["gpt-4o-mini"]
}

agent number_note for n in [1, 2, 3, 4] {
    model: openai("gpt-4o-mini")
    prompt: "Number {{ n }}"

    output: {
        number: number
        note: string
    }
}

output {
    notes: agent.number_note
}
```
