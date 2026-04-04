```ai
agent formatter_checks {
    prompt: "This is a very long prompt sentence that should exceed the formatter line width limit and therefore be wrapped into a multiline string block automatically by the formatter."
    short_numbers: [1,2,3]
    long_numbers: [1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24,25,26,27,28,29,30]
    output: string
}

output { value: agent.formatter_checks }
```
---
```ai
agent formatter_checks {
    prompt: """
        This is a very long prompt sentence that should exceed the formatter line width limit and therefore be wrapped
        into a multiline string block automatically by the formatter.
    """
    short_numbers: [1, 2, 3]
    long_numbers: [
        1,
        2,
        3,
        4,
        5,
        6,
        7,
        8,
        9,
        10,
        11,
        12,
        13,
        14,
        15,
        16,
        17,
        18,
        19,
        20,
        21,
        22,
        23,
        24,
        25,
        26,
        27,
        28,
        29,
        30,
    ]
    output: string
}

output {
    value: agent.formatter_checks
}
```
