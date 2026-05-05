```wire
mcp local{endpoint:"https://mcp.example.test/rpc"}
input{audience:string topic:string}
prompt system_prompt from mcp.local.prompt.system-prompt{bindings{audience:input.audience}}
dynamic{instructions:render prompt.system_prompt{params{topic:input.topic}}}
output{instructions:dynamic.instructions}
```
---
```wire
mcp local {
    endpoint: "https://mcp.example.test/rpc"
}

input {
    audience: string
    topic: string
}

prompt system_prompt from mcp.local.prompt.system-prompt {
    bindings {
        audience: input.audience
    }
}

dynamic {
    instructions: render prompt.system_prompt {
        params {
            topic: input.topic
        }
    }
}

output {
    instructions: dynamic.instructions
}
```
