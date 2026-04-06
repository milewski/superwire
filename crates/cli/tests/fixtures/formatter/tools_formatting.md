```wire
agent tool_user {
    tools:[tool.name,tool.name2(some_property:123,another:456),tool.name3(really_long_properties_name:123,another_really_long_property_name:456,third_really_long_property_name:789,fourth_really_long_property_name:101112)]
    prompt:"Use tools"
    output:string
}

output { value: agent.tool_user }
```
---
```wire
agent tool_user {
    tools: [
        tool.name,
        tool.name2(some_property: 123, another: 456),
        tool.name3(
            really_long_properties_name: 123,
            another_really_long_property_name: 456,
            third_really_long_property_name: 789,
            fourth_really_long_property_name: 101112,
        ),
    ]

    prompt: "Use tools"
    output: string
}

output {
    value: agent.tool_user
}
```
