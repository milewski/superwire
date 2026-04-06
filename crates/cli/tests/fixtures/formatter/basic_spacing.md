```wire
provider  openai   {driver :  "openai" models:["gpt-4o-mini",]}
   output {   result:"ok" }
```
---
```wire
provider openai {
    driver: "openai"
    models: ["gpt-4o-mini"]
}

output {
    result: "ok"
}
```
