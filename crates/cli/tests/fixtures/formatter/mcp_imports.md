```wire
mcp local{endpoint:"https://mcp.example.test/rpc"}
input{workspace_id:string project_id:string}
resource project_readme from mcp.local.resource.project-readme{params{workspace_id:input.workspace_id project_id:input.project_id}}
prompt from mcp.local.prompt.system-prompt
tool create_sorting_task_for_task_group from mcp.local.tool.create-sorting-task-for-task-group-tool{bindings{workspace_id:input.workspace_id project_id:input.project_id}}
dynamic{readme:read resource.project_readme{params{section:"setup"}} instructions:render prompt.system_prompt{bindings{readme:dynamic.readme}}}
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

resource project_readme from mcp.local.resource.project-readme {
    bindings {
        workspace_id: input.workspace_id
        project_id: input.project_id
    }
}

prompt system_prompt from mcp.local.prompt.system-prompt

tool create_sorting_task_for_task_group from mcp.local.tool.create-sorting-task-for-task-group-tool {
    bindings {
        workspace_id: input.workspace_id
        project_id: input.project_id
    }
}

dynamic {
    readme: read resource.project_readme {
        params {
            section: "setup"
        }
    }
    instructions: render prompt.system_prompt {
        params {
            readme: dynamic.readme
        }
    }
}
```
