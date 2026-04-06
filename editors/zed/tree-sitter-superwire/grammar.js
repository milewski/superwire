module.exports = grammar({
  name: "superwire",

  extras: ($) => [
    /\s/,
    $.comment,
  ],

  word: ($) => $.identifier,

  rules: {
    source_file: ($) => repeat($._statement),

    _statement: ($) => choice(
      $.provider_declaration,
      $.schema_declaration,
      $.agent_declaration,
      $.secrets_block,
      $.input_block,
      $.output_block,
      $.property_assignment,
      $.expression_statement,
    ),

    provider_declaration: ($) => seq(
      "provider",
      field("name", $.identifier),
      field("body", $.block),
    ),

    schema_declaration: ($) => seq(
      "schema",
      field("name", $.identifier),
      field("body", $.block),
    ),

    agent_declaration: ($) => seq(
      "agent",
      field("name", $.identifier),
      optional(seq(
        "for",
        field("iterator", $.identifier),
        "in",
        field("iterable", $._expression),
      )),
      field("body", $.block),
    ),

    secrets_block: ($) => seq("secrets", field("body", $.block)),
    input_block: ($) => seq("input", field("body", $.block)),
    output_block: ($) => seq("output", field("body", $.block)),

    block: ($) => seq("{", repeat($._block_item), "}"),

    _block_item: ($) => choice(
      $.provider_declaration,
      $.schema_declaration,
      $.agent_declaration,
      $.secrets_block,
      $.input_block,
      $.output_block,
      $.property_assignment,
      $.expression_statement,
    ),

    property_assignment: ($) => prec(2, seq(
      field("key", choice($.identifier, $.keyword_identifier)),
      ":",
      field("value", $._expression),
      optional(choice(",", ";")),
    )),

    expression_statement: ($) => seq($._expression, optional(choice(",", ";"))),

    _expression: ($) => choice(
      $.binary_expression,
      $._primary_expression,
    ),

    binary_expression: ($) => prec.left(seq(
      $._primary_expression,
      repeat1(seq("|", $._primary_expression)),
    )),

    _primary_expression: ($) => choice(
      $.object,
      $.array,
      $.function_call,
      $.reference,
      $.triple_string,
      $.string,
      $.number,
      $.boolean,
      $.identifier,
    ),

    object: ($) => seq("{", repeat(choice($.property_assignment, $.expression_statement)), "}"),

    array: ($) => seq("[", optional(seq($._expression, repeat(seq(",", $._expression)), optional(","))), "]"),

    function_call: ($) => prec.left(seq(
      field("function", choice($.namespaced_identifier, $.identifier)),
      field("arguments", $.arguments),
    )),

    arguments: ($) => seq("(", optional(seq($.argument, repeat(seq(",", $.argument)), optional(","))), ")"),

    argument: ($) => choice($.named_argument, $._expression),

    named_argument: ($) => seq(
      field("name", $.identifier),
      ":",
      field("value", $._expression),
    ),

    namespaced_identifier: ($) => prec.left(seq(
      field("namespace", $.identifier),
      ".",
      field("name", $.identifier),
    )),

    reference: ($) => prec.left(seq(
      field("root", $.identifier),
      repeat1(seq(
        field("operator", choice(".", "?.")),
        field("property", $.identifier),
      )),
    )),

    identifier: (_$) => /[A-Za-z_][A-Za-z0-9_]*/,

    keyword_identifier: (_$) => choice(
      "input",
      "output",
      "secrets",
      "context",
      "string",
      "number",
      "float",
      "boolean",
      "null",
      "tool",
    ),

    number: (_$) => /\d(?:[\d_]*\d)?(?:\.\d(?:[\d_]*\d)?)?/,

    boolean: (_$) => choice("true", "false"),

    string: (_$) => token(seq(
      '"',
      repeat(choice(/[^"\\]/, /\\./)),
      '"',
    )),

    triple_string: (_$) => token(seq(
      '"""',
      /[^\"]*/,
      '"""',
    )),

    comment: (_$) => token(seq("//", /.*/)),
  },
});
