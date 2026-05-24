```wire
agent redact_notes {
    model: model.ollama_model
    instruction: "Redact names, emails, and phone numbers from these interview notes: {{ input.interview_notes }}"
    output {
        redacted_notes: [string]
        redaction_summary: string
    }
}
```
---
```wire
agent redact_notes {
    model: model.ollama_model
    instruction: "Redact names, emails, and phone numbers from these interview notes: {{ input.interview_notes }}"
    output {
        redacted_notes: [string]
        redaction_summary: string
    }
}
```
