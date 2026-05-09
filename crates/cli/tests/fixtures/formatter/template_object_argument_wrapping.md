```wire
input {
    study_name: string
    audience: string
    findings: [string]
}

agent research_single_entry {
    model: openai("gpt-4.1-mini")
    instruction: template( "prompts/research_brief.md", { study_name: input.study_name } )
    output: string
}

agent research_multi_entry {
    model: openai("gpt-4.1-mini")
    instruction: template("prompts/research_brief.md", { study_name: input.study_name audience: input.audience findings: input.findings })
    output: string
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
    model: openai("gpt-4.1-mini")
    instruction: template("prompts/research_brief.md", { study_name: input.study_name })
    output: string
}

agent research_multi_entry {
    model: openai("gpt-4.1-mini")

    instruction: template("prompts/research_brief.md", {
        study_name: input.study_name
        audience: input.audience
        findings: input.findings
    })

    output: string
}
```
