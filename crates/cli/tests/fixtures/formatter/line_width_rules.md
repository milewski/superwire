```wire
agent formatter_checks {
    model: openai("gpt-4.1-mini")
    prompt: "This is a very long prompt sentence that should exceed the formatter line width limit and therefore be wrapped into a multiline string block automatically by the formatter."
    context: [1,2,3]
    tools: [tool.one,tool.two,tool.three,tool.four,tool.five,tool.six,tool.seven,tool.eight,tool.nine,tool.ten,tool.eleven,tool.twelve,tool.thirteen,tool.fourteen,tool.fifteen]
    output: string
}

output { value: agent.formatter_checks }
```
---
```wire
agent formatter_checks {
    model: openai("gpt-4.1-mini")

    prompt: """
        This is a very long prompt sentence that should exceed the formatter line width limit and therefore be wrapped
        into a multiline string block automatically by the formatter.
    """

    context: [1, 2, 3]

    tools: [
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

    output: string
}

output {
    value: agent.formatter_checks
}
```
