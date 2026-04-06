(comment) @comment

[
  "provider"
  "schema"
  "agent"
  "for"
  "in"
  "input"
  "output"
  "secrets"
] @keyword

(provider_declaration
  name: (identifier) @type)

(schema_declaration
  name: (identifier) @type)

(agent_declaration
  name: (identifier) @function)

(property_assignment
  key: (identifier) @property)

(named_argument
  name: (identifier) @variable.parameter)

(function_call
  function: (identifier) @function)

(function_call
  function: (namespaced_identifier
    namespace: (identifier) @type
    name: (identifier) @function))

(reference
  root: (identifier) @variable.special
  (#any-of? @variable.special "agent" "input" "schema" "tool" "secrets"))

(reference
  property: (identifier) @variable)

(number) @number
(boolean) @boolean

[
  (string)
  (triple_string)
] @string

[
  "|"
  "."
  "?."
] @operator

[
  "{"
  "}"
  "["
  "]"
  "("
  ")"
] @punctuation.bracket

[
  ","
  ":"
  ";"
] @punctuation.delimiter
