```wire
input {
    study_name: string
    audience: string
    findings: [string]
}

agent research_single_entry {
    model: model.openai_model
    instruction: template( "prompts/research_brief.md", { study_name: input.study_name } )
    output {
        value: string
    }
}

agent research_multi_entry {
    model: model.openai_model
    instruction: template("prompts/research_brief.md", { study_name: input.study_name audience: input.audience findings: input.findings })
    output {
        value: string
    }
}
```
---
```wire
input {
    study_name: string
    audience: string
    findings: [string]
}

agent research_single_entry {
    model: model.openai_model
    instruction: template("prompts/research_brief.md", { study_name: input.study_name })
    output {
        value: string
    }
}

agent research_multi_entry {
    model: model.openai_model

    instruction: template("prompts/research_brief.md", {
        study_name: input.study_name
        audience: input.audience
        findings: input.findings
    })

    output {
        value: string
    }
}
```
