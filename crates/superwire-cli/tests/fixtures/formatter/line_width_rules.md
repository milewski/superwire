```wire
agent formatter_checks {
    model: model.openai_model
    instruction: "This is a very long prompt sentence that should exceed the formatter line width limit and therefore be wrapped into a multiline string block automatically by the formatter."
    uses: [tool.one,tool.two,tool.three,tool.four,tool.five,tool.six,tool.seven,tool.eight,tool.nine,tool.ten,tool.eleven,tool.twelve,tool.thirteen,tool.fourteen,tool.fifteen]
    output {
        value: string
    }
}

output { value: agent.formatter_checks.value }
```
---
```wire
agent formatter_checks {
    model: model.openai_model

    instruction: """
        This is a very long prompt sentence that should exceed the formatter line width limit and therefore be wrapped
        into a multiline string block automatically by the formatter.
    """

    uses: [
        tool.one,
        tool.two,
        tool.three,
        tool.four,
        tool.five,
        tool.six,
        tool.seven,
        tool.eight,
        tool.nine,
        tool.ten,
        tool.eleven,
        tool.twelve,
        tool.thirteen,
        tool.fourteen,
        tool.fifteen,
    ]

    output {
        value: string
    }
}

output {
    value: agent.formatter_checks.value
}
```
