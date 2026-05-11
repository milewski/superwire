```wire
input { topic:string "Main topic" count:number "How many outputs" }

agent writer {instruction:"Write" output:{summary:string "One-line summary" ok:boolean "Status flag"}}

output { result:agent.writer }
```
---
```wire
input {
    /// Main topic
    topic: string
    /// How many outputs
    count: number
}

agent writer {
    instruction: "Write"
    output: {
        /// One-line summary
        summary: string
        /// Status flag
        ok: boolean
    }
}

output {
    result: agent.writer
}
```
