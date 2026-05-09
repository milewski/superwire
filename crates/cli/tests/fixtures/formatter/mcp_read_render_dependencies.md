```wire
mcp local{endpoint:"https://mcp.example.test/rpc"}
input{workspace_id:string topic:string}
resource topic_notes from mcp.local.resource.topic_notes{bindings{workspace_id:input.workspace_id}}
prompt reviewer_prompt from mcp.local.prompt.reviewer_prompt{bindings{topic:input.topic}}
dynamic{notes:read resource.topic_notes{bindings{topic:input.topic}} review_instruction:render prompt.reviewer_prompt{bindings{notes:dynamic.notes}}}
agent reviewer{model:openai("gpt-4.1-mini") instruction:"Review {{ dynamic.review_prompt }} with {{ dynamic.notes }}" output:{summary:string}}
provider openai{driver:"openai" endpoint:"https://api.openai.com/v1" api_key:"test-api-key" models:["gpt-4.1-mini"]}
output{notes:dynamic.notes review:agent.reviewer.summary}
```
---
```wire
mcp local {
    endpoint: "https://mcp.example.test/rpc"
}

input {
    workspace_id: string
    topic: string
}

resource topic_notes from mcp.local.resource.topic_notes {
    bindings {
        workspace_id: input.workspace_id
    }
}

prompt reviewer_prompt from mcp.local.prompt.reviewer_prompt {
    bindings {
        topic: input.topic
    }
}

dynamic {
    notes: read resource.topic_notes {
        bindings {
            topic: input.topic
        }
    }
    review_instruction: render prompt.reviewer_prompt {
        bindings {
            notes: dynamic.notes
        }
    }
}

agent reviewer {
    model: openai("gpt-4.1-mini")
    instruction: "Review {{ dynamic.review_prompt }} with {{ dynamic.notes }}"
    output: {
        summary: string
    }
}

provider openai {
    driver: "openai"
    endpoint: "https://api.openai.com/v1"
    api_key: "test-api-key"
    models: ["gpt-4.1-mini"]
}

output {
    notes: dynamic.notes
    review: agent.reviewer.summary
}
```
