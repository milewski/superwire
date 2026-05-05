```wire
mcp local{endpoint:"https://mcp.example.test/rpc"}
input{workspace_id:string topic:string}
resource topic_notes from mcp.local.resource.topic-notes{bindings{workspace_id:input.workspace_id}}
prompt reviewer_prompt from mcp.local.prompt.reviewer-prompt{bindings{topic:input.topic}}
dynamic{notes:read resource.topic_notes{params{topic:input.topic}} review_prompt:render prompt.reviewer_prompt{params{notes:dynamic.notes}}}
agent reviewer{model:openai("gpt-4.1-mini") prompt:"Review {{ dynamic.review_prompt }} with {{ dynamic.notes }}" output:{summary:string}}
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

resource topic_notes from mcp.local.resource.topic-notes {
    bindings {
        workspace_id: input.workspace_id
    }
}

prompt reviewer_prompt from mcp.local.prompt.reviewer-prompt {
    bindings {
        topic: input.topic
    }
}

dynamic {
    notes: read resource.topic_notes {
        params {
            topic: input.topic
        }
    }
    review_prompt: render prompt.reviewer_prompt {
        params {
            notes: dynamic.notes
        }
    }
}

agent reviewer {
    model: openai("gpt-4.1-mini")
    prompt: "Review {{ dynamic.review_prompt }} with {{ dynamic.notes }}"
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
