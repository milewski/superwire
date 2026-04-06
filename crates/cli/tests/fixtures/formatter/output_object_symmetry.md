```wire
agent redact_notes {
    model: ollama("qwen3.5:8b")
    prompt: "Redact names, emails, and phone numbers from these interview notes: {{ input.interview_notes }}"
    output: {
        redacted_notes: [string]
        redaction_summary: string
    }
}
```
---
```wire
agent redact_notes {
    model: ollama("qwen3.5:8b")
    prompt: "Redact names, emails, and phone numbers from these interview notes: {{ input.interview_notes }}"
    output: {
        redacted_notes: [string]
        redaction_summary: string
    }
}
```
