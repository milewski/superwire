```wire
provider  openai   from openai {}
model openai_model from openai {id:"gpt-4o-mini"}
   output {   result:"ok" }
```
---
```wire
provider openai from openai {
}

model openai_model from openai {
    id: "gpt-4o-mini"
}

output {
    result: "ok"
}
```
