```ai
input { topic:string "Main topic" count:number "How many outputs" }

agent writer {prompt:"Write" output:{summary:string "One-line summary" ok:boolean "Status flag"}}

output { result:agent.writer }
```
---
```ai
input {
    topic: string "Main topic"
    count: number "How many outputs"
}

agent writer {
    prompt: "Write"

    output: {
        summary: string "One-line summary"
        ok: boolean "Status flag"
    }
}

output {
    result: agent.writer
}
```
