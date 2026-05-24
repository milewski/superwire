```wire
mcp local{endpoint:"https://mcp.example.test/rpc"}
input{workspace_id:string section:string}
resource project_readme from mcp.local.resource.project_readme{bindings{workspace_id:input.workspace_id}}
dynamic{readme:read resource.project_readme{bindings{section:input.section}}}
output{readme:dynamic.readme}
```
---
```wire
mcp local {
    endpoint: "https://mcp.example.test/rpc"
}

input {
    workspace_id: string
    section: string
}

resource project_readme from mcp.local.resource.project_readme {
    bindings {
        workspace_id: input.workspace_id
    }
}

dynamic {
    readme: read resource.project_readme {
        bindings {
            section: input.section
        }
    }
}

output {
    readme: dynamic.readme
}
```
