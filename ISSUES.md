- [ ] Add agent property contract validation (required properties, duplicate properties, and property shape checks)
- [ ] Add provider contract validation (`driver` type/required checks, `models` shape, duplicate model entries)
- [ ] Tighten `model:` call validation semantics (single source of model name, disallow ambiguous argument combinations)
- [ ] Add source spans (line/column) to parse and validation diagnostics for better LSP and CLI error reporting
- [ ] Improve cycle diagnostics to include at least one concrete cycle path (not only node set)
- [ ] Create a builder for generating workflow files 
      ```rust
      let workflow = TestWorkflowFixture::default()
           .single_agent("number_agent")
           .prompt("return a number")
           .agent_output_number()
           .output_ref("answer", "agent.number_agent")
           .build();
      ```