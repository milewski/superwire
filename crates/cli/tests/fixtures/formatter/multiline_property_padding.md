```wire
agent greeting_single {
    model: ollama("qwen3.5:8b")
    instruction: "aaa"
    output: string
}

agent greeting_prompt_multiline {
    model: ollama("qwen3.5:8b")
    instruction: """
        test
    """
    output: string
}

agent greeting_inference_multiline {
    model: ollama("qwen3.5:8b")
    inference: {        temperature: 0.7}
    instruction: "test"
    output: string
}

agent greeting_tools_then_inference {
    model: ollama("qwen3.5:8b")
    uses: [tool.calculator]
    inference: {temperature: 0.7}
    instruction: "test"
    output: string
}

agent greeting_multiline_tools_then_inference {
    model: ollama("qwen3.5:8b")
    uses: [tool.calculator1,tool.calculator2,tool.calculator3,tool.calculator4,tool.calculator5,tool.calculator1,tool.calculator2,tool.calculator3,tool.calculator4,tool.calculator5]
    inference: {temperature: 0.7}
    instruction: "test"
    output: string
}
```
---
```wire
agent greeting_single {
    model: ollama("qwen3.5:8b")
    instruction: "aaa"
    output: string
}

agent greeting_prompt_multiline {
    model: ollama("qwen3.5:8b")

    instruction: """
        test
    """

    output: string
}

agent greeting_inference_multiline {
    model: ollama("qwen3.5:8b")
    inference: { temperature: 0.7 }
    instruction: "test"
    output: string
}

agent greeting_tools_then_inference {
    model: ollama("qwen3.5:8b")
    uses: [tool.calculator]
    inference: { temperature: 0.7 }
    instruction: "test"
    output: string
}

agent greeting_multiline_tools_then_inference {
    model: ollama("qwen3.5:8b")

    uses: [
        tool.calculator1,
        tool.calculator2,
        tool.calculator3,
        tool.calculator4,
        tool.calculator5,
        tool.calculator1,
        tool.calculator2,
        tool.calculator3,
        tool.calculator4,
        tool.calculator5,
    ]

    inference: { temperature: 0.7 }
    instruction: "test"
    output: string
}
```
