```wire
mcp local{endpoint:"https://mcp.example.test/rpc"}
input{workspace_id:string project_id:string}
resource project_readme from mcp.local.resource.project_readme{bindings{workspace_id:input.workspace_id project_id:input.project_id}}
prompt system_prompt from mcp.local.prompt.system_prompt
tool create_sorting_task_for_task_group from mcp.local.tool.create_sorting_task_for_task_group_tool{bindings{workspace_id:input.workspace_id project_id:input.project_id}}
dynamic{readme:read resource.project_readme{bindings{section:"setup"}} instructions:render prompt.system_prompt{bindings{readme:dynamic.readme}}}
```
---
```wire
mcp local {
    endpoint: "https://mcp.example.test/rpc"
}

input {
    workspace_id: string
    project_id: string
}

resource project_readme from mcp.local.resource.project_readme {
    bindings {
        workspace_id: input.workspace_id
        project_id: input.project_id
    }
}

prompt system_prompt from mcp.local.prompt.system_prompt

tool create_sorting_task_for_task_group from mcp.local.tool.create_sorting_task_for_task_group_tool {
    bindings {
        workspace_id: input.workspace_id
        project_id: input.project_id
    }
}

dynamic {
    readme: read resource.project_readme {
        bindings {
            section: "setup"
        }
    }
    instructions: render prompt.system_prompt {
        bindings {
            readme: dynamic.readme
        }
    }
}
```
