```wire
input { topic:string "Main topic" count:number "How many outputs" }

agent writer {instruction:"Write" output:{summary:string "One-line summary" ok:boolean "Status flag"}}

output { result:agent.writer }
```
---
```wire
input {
    topic: string "Main topic"
    count: number "How many outputs"
}

agent writer {
    instruction: "Write"
    output: {
        summary: string "One-line summary"
        ok: boolean "Status flag"
    }
}

output {
    result: agent.writer
}
```
