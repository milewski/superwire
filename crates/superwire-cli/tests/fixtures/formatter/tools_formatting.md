```wire
agent tool_user {
    uses:[tool.name,tool.name2 { bindings { some_property:123 another:456 } },tool.name3 { bindings { really_long_properties_name:123 another_really_long_property_name:456 third_really_long_property_name:789 fourth_really_long_property_name:101112 } }]
    instruction:"Use tools"
    output{value:string}
}

output { value: agent.tool_user.value }
```
---
```wire
agent tool_user {
    uses: [
        tool.name,
        tool.name2 {
            bindings {
                some_property: 123
                another: 456
            }
        },
        tool.name3 {
            bindings {
                really_long_properties_name: 123
                another_really_long_property_name: 456
                third_really_long_property_name: 789
                fourth_really_long_property_name: 101112
            }
        },
    ]

    instruction: "Use tools"
    output {
        value: string
    }
}

output {
    value: agent.tool_user.value
}
```
