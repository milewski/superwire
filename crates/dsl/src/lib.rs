#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymbolCategory {
    Keyword,
    Function,
    Namespace,
    Property,
    Type,
}

#[derive(Debug, Clone, Copy)]
pub struct SymbolDoc {
    pub label: &'static str,
    pub category: SymbolCategory,
    pub detail: &'static str,
    pub documentation: &'static str,
}

const BUILTIN_SYMBOLS: &[SymbolDoc] = &[
    SymbolDoc {
        label: "provider",
        category: SymbolCategory::Keyword,
        detail: "Declare a model provider",
        documentation: "Defines a named provider block with driver configuration and available models.",
    },
    SymbolDoc {
        label: "schema",
        category: SymbolCategory::Keyword,
        detail: "Declare a reusable schema",
        documentation: "Defines a named schema that can later be referenced as `schema.Name`.",
    },
    SymbolDoc {
        label: "agent",
        category: SymbolCategory::Keyword,
        detail: "Declare a workflow agent",
        documentation: "Defines an executable workflow step with model, prompt, context, and output.",
    },
    SymbolDoc {
        label: "input",
        category: SymbolCategory::Keyword,
        detail: "Declare workflow input",
        documentation: "Defines the external values a workflow expects before execution.",
    },
    SymbolDoc {
        label: "output",
        category: SymbolCategory::Keyword,
        detail: "Declare workflow output",
        documentation: "Builds the final object returned by the workflow.",
    },
    SymbolDoc {
        label: "secrets",
        category: SymbolCategory::Keyword,
        detail: "Declare secret inputs",
        documentation: "Defines secret values that can be injected into provider or tool configuration.",
    },
    SymbolDoc {
        label: "for",
        category: SymbolCategory::Keyword,
        detail: "Start a looped agent declaration",
        documentation: "Creates one agent execution per item in the input collection.",
    },
    SymbolDoc {
        label: "in",
        category: SymbolCategory::Keyword,
        detail: "Loop source separator",
        documentation: "Separates the loop variable from the iterable expression in `agent ... for value in expr`.",
    },
    SymbolDoc {
        label: "context",
        category: SymbolCategory::Function,
        detail: "Return full agent context",
        documentation: "Extracts the full execution context from a prior agent so it can be reused by a later step.",
    },
    SymbolDoc {
        label: "compact",
        category: SymbolCategory::Function,
        detail: "Return compacted agent context",
        documentation: "Compacts a prior agent context into a smaller representation. It can accept named arguments like `agent:`, `model:`, `inference:`, and `prompt:`.",
    },
    SymbolDoc {
        label: "template",
        category: SymbolCategory::Function,
        detail: "Load a prompt template",
        documentation: "Loads a template file and binds named values into it for prompt construction.",
    },
    SymbolDoc {
        label: "agent",
        category: SymbolCategory::Namespace,
        detail: "Agent reference namespace",
        documentation: "Use `agent.name` or `agent.name.field` to reference outputs from previous agents.",
    },
    SymbolDoc {
        label: "input",
        category: SymbolCategory::Namespace,
        detail: "Input reference namespace",
        documentation: "Use `input.field` to reference workflow input values.",
    },
    SymbolDoc {
        label: "schema",
        category: SymbolCategory::Namespace,
        detail: "Schema type namespace",
        documentation: "Use `schema.Name` to reference a named schema type.",
    },
    SymbolDoc {
        label: "tool",
        category: SymbolCategory::Namespace,
        detail: "Tool reference namespace",
        documentation: "Use `tool.name` or `tool.name(...)` to reference runtime-provided tools.",
    },
    SymbolDoc {
        label: "secrets",
        category: SymbolCategory::Namespace,
        detail: "Secrets reference namespace",
        documentation: "Use `secrets.name` to reference a declared secret value.",
    },
    SymbolDoc {
        label: "model",
        category: SymbolCategory::Property,
        detail: "Agent model property",
        documentation: "Selects the provider model used by an agent execution.",
    },
    SymbolDoc {
        label: "prompt",
        category: SymbolCategory::Property,
        detail: "Agent prompt property",
        documentation: "Defines the prompt string or template used by an agent.",
    },
    SymbolDoc {
        label: "context",
        category: SymbolCategory::Property,
        detail: "Agent context property",
        documentation: "Provides a prior agent context to a new agent, usually via `context(...)` or `compact(...)`.",
    },
    SymbolDoc {
        label: "output",
        category: SymbolCategory::Property,
        detail: "Output shape property",
        documentation: "Defines the output type or inline schema produced by an agent.",
    },
    SymbolDoc {
        label: "tools",
        category: SymbolCategory::Property,
        detail: "Tool list property",
        documentation: "Lists the tools an agent may call during execution.",
    },
    SymbolDoc {
        label: "inference",
        category: SymbolCategory::Property,
        detail: "Inference settings property",
        documentation: "Configures model generation settings like temperature and max tokens.",
    },
    SymbolDoc {
        label: "driver",
        category: SymbolCategory::Property,
        detail: "Provider driver property",
        documentation: "Selects the backend driver implementation for a provider.",
    },
    SymbolDoc {
        label: "endpoint",
        category: SymbolCategory::Property,
        detail: "Provider endpoint property",
        documentation: "Overrides the provider API endpoint.",
    },
    SymbolDoc {
        label: "api_key",
        category: SymbolCategory::Property,
        detail: "Provider API key property",
        documentation: "Provides authentication credentials for a provider.",
    },
    SymbolDoc {
        label: "models",
        category: SymbolCategory::Property,
        detail: "Provider models property",
        documentation: "Lists the models exposed by a provider.",
    },
    SymbolDoc {
        label: "string",
        category: SymbolCategory::Type,
        detail: "String type",
        documentation: "Represents a UTF-8 string value.",
    },
    SymbolDoc {
        label: "number",
        category: SymbolCategory::Type,
        detail: "Number type",
        documentation: "Represents a numeric value.",
    },
    SymbolDoc {
        label: "float",
        category: SymbolCategory::Type,
        detail: "Float type",
        documentation: "Represents a floating-point numeric value.",
    },
    SymbolDoc {
        label: "boolean",
        category: SymbolCategory::Type,
        detail: "Boolean type",
        documentation: "Represents `true` or `false`.",
    },
    SymbolDoc {
        label: "null",
        category: SymbolCategory::Type,
        detail: "Null type",
        documentation: "Represents an explicit null value.",
    },
];

#[must_use]
pub fn builtin_symbols() -> &'static [SymbolDoc] {
    BUILTIN_SYMBOLS
}

#[must_use]
pub fn lookup_symbol(symbol: &str) -> Option<&'static SymbolDoc> {
    BUILTIN_SYMBOLS.iter().find(|candidate_symbol| candidate_symbol.label == symbol)
}
