```wire
input { 
/// Main topic
topic:string
/// How many outputs
count:number }

agent writer {instruction:"Write" output:{
/// One-line summary
summary:string
/// Status flag
ok:boolean}}

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
