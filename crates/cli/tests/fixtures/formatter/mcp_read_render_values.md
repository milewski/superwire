```wire
mcp local{endpoint:"https://mcp.example.test/rpc"}
input{workspace_id:string}
resource project_readme from mcp.local.resource.project_readme{bindings{workspace_id:input.workspace_id}}
prompt system_prompt from mcp.local.prompt.system_prompt
dynamic{readme:read resource.project_readme{bindings{section:"setup"}} instructions:render prompt.system_prompt{bindings{readme:dynamic.readme audience:"maintainers"}}}
output{readme:dynamic.readme instructions:dynamic.instructions}
```
---
```wire
mcp local {
    endpoint: "https://mcp.example.test/rpc"
}

input {
    workspace_id: string
}

resource project_readme from mcp.local.resource.project_readme {
    bindings {
        workspace_id: input.workspace_id
    }
}

prompt system_prompt from mcp.local.prompt.system_prompt

dynamic {
    readme: read resource.project_readme {
        bindings {
            section: "setup"
        }
    }
    instructions: render prompt.system_prompt {
        bindings {
            readme: dynamic.readme
            audience: "maintainers"
        }
    }
}

output {
    readme: dynamic.readme
    instructions: dynamic.instructions
}
```
