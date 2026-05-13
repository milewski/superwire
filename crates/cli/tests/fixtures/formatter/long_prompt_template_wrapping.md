```wire
input { topic: string }

agent long_template_prompt {
    model: model.ollama_model
    instruction: "This prompt includes {{ input.topic }} and continues with additional explanatory wording that should exceed the configured line width so it cannot stay on one line."
    output {
        value: string
    }
}

agent long_multiline_prompt {
    model: model.ollama_model
    instruction: """This multiline prompt line is intentionally very long and should be wrapped by the formatter so each rendered content line stays at or under the configured width limit."""
    output {
        value: string
    }
}
```
---
```wire
input {
    topic: string
}

agent long_template_prompt {
    model: model.ollama_model

    instruction: """
        This prompt includes {{ input.topic }} and continues with additional explanatory wording that should exceed the
        configured line width so it cannot stay on one line.
    """

    output {
        value: string
    }
}

agent long_multiline_prompt {
    model: model.ollama_model

    instruction: """
        This multiline prompt line is intentionally very long and should be wrapped by the formatter so each rendered
        content line stays at or under the configured width limit.
    """

    output {
        value: string
    }
}
```
