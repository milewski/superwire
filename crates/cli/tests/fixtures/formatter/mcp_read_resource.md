```wire
mcp local{endpoint:"https://mcp.example.test/rpc"}
input{workspace_id:string section:string}
resource project_readme from mcp.local.resource.project-readme{bindings{workspace_id:input.workspace_id}}
dynamic{readme:read resource.project_readme{params{section:input.section}}}
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

resource project_readme from mcp.local.resource.project-readme {
    bindings {
        workspace_id: input.workspace_id
    }
}

dynamic {
    readme: read resource.project_readme {
        params {
            section: input.section
        }
    }
}

output {
    readme: dynamic.readme
}
```
