use crate::dsl::ast::{TypeExpression, TypedField};

use super::wrapping::render_plain_string_literal;
use super::DslFormatter;

impl TypedField {
    pub(super) fn push_to_formatter(&self, formatter: &mut DslFormatter) {
        if let Some(description) = &self.description {
            for description_line in description.lines() {
                formatter.push_indent();
                formatter.output.push_str("///");

                if !description_line.is_empty() {
                    formatter.output.push(' ');
                    formatter.output.push_str(description_line);
                }

                formatter.push_newline();
            }
        }

        formatter.push_indent();
        formatter.output.push_str(&self.name);
        formatter.output.push_str(": ");
        self.field_type.push_to_formatter(formatter);

        formatter.push_newline();
    }
}

impl TypeExpression {
    pub(super) fn push_to_formatter(&self, formatter: &mut DslFormatter) {
        match self {
            Self::String => formatter.output.push_str("string"),
            Self::Number => formatter.output.push_str("number"),
            Self::Float => formatter.output.push_str("float"),
            Self::Boolean => formatter.output.push_str("boolean"),
            Self::Null => formatter.output.push_str("null"),
            Self::AnyObject => formatter.output.push_str("object"),
            Self::SchemaReference(schema_name) => {
                formatter.output.push_str("schema.");
                formatter.output.push_str(schema_name);
            }
            Self::StringEnum(enum_value) => formatter.output.push_str(&render_plain_string_literal(enum_value)),
            Self::StringEnumReference(reference) => reference.push_to_formatter(formatter),
            Self::Array { item_type, fixed_length } => {
                if item_type.should_break_inside_array() {
                    formatter.output.push('[');
                    formatter.push_newline();
                    formatter.indentation_depth += 1;
                    formatter.push_indent();
                    item_type.push_to_formatter(formatter);

                    if let Some(array_length) = fixed_length {
                        formatter.output.push_str("; ");
                        formatter.output.push_str(&array_length.to_string());
                    }

                    formatter.push_newline();
                    formatter.indentation_depth -= 1;
                    formatter.push_indent();
                    formatter.output.push(']');

                    return;
                }

                formatter.output.push('[');
                item_type.push_to_formatter(formatter);

                if let Some(array_length) = fixed_length {
                    formatter.output.push_str("; ");
                    formatter.output.push_str(&array_length.to_string());
                }

                formatter.output.push(']');
            }
            Self::Tuple(tuple_items) => {
                formatter.output.push('(');
                let mut tuple_item_iterator = tuple_items.iter().peekable();

                while let Some(tuple_item) = tuple_item_iterator.next() {
                    tuple_item.push_to_formatter(formatter);

                    if tuple_item_iterator.peek().is_some() {
                        formatter.output.push_str(", ");
                    }
                }

                formatter.output.push(')');
            }
            Self::Object(object_fields) => {
                formatter.output.push('{');
                formatter.push_newline();
                formatter.indentation_depth += 1;

                for typed_field in object_fields {
                    typed_field.push_to_formatter(formatter);
                }

                formatter.indentation_depth -= 1;
                formatter.push_indent();
                formatter.output.push('}');
            }
            Self::Variant { discriminator, cases } => {
                formatter.output.push_str("variant ");
                formatter.output.push_str(discriminator);
                formatter.output.push_str(" {");
                formatter.push_newline();
                formatter.indentation_depth += 1;

                for variant_case in cases {
                    formatter.push_indent();
                    formatter.output.push_str(&variant_case.name);
                    formatter.output.push_str(" {");
                    formatter.push_newline();
                    formatter.indentation_depth += 1;

                    for typed_field in &variant_case.fields {
                        typed_field.push_to_formatter(formatter);
                    }

                    formatter.indentation_depth -= 1;
                    formatter.push_indent();
                    formatter.output.push('}');
                    formatter.push_newline();
                }

                formatter.indentation_depth -= 1;
                formatter.push_indent();
                formatter.output.push('}');
            }
            Self::Union(union_members) => {
                if Self::push_nullable_union_to_formatter(union_members, formatter) {
                    return;
                }

                if Self::push_string_enum_union_to_formatter(union_members, formatter) {
                    return;
                }

                let mut union_member_iterator = union_members.iter().peekable();

                while let Some(union_member) = union_member_iterator.next() {
                    union_member.push_to_formatter(formatter);

                    if union_member_iterator.peek().is_some() {
                        formatter.output.push_str(" | ");
                    }
                }
            }
        }
    }

    fn push_nullable_union_to_formatter(union_members: &[Self], formatter: &mut DslFormatter) -> bool {
        if !union_members.iter().any(|union_member| matches!(union_member, Self::Null)) {
            return false;
        }

        let non_null_members = union_members
            .iter()
            .filter(|union_member| !matches!(union_member, Self::Null))
            .collect::<Vec<_>>();

        if non_null_members.len() == 1 {
            formatter.output.push_str("maybe ");
            non_null_members[0].push_to_formatter(formatter);

            return true;
        }

        if non_null_members
            .iter()
            .all(|union_member| matches!(union_member, Self::StringEnum(_)))
        {
            formatter.output.push_str("maybe ");
            Self::push_string_enum_members_to_formatter(non_null_members.as_slice(), formatter);

            return true;
        }

        false
    }

    fn push_string_enum_union_to_formatter(union_members: &[Self], formatter: &mut DslFormatter) -> bool {
        if !union_members.iter().all(|union_member| matches!(union_member, Self::StringEnum(_))) {
            return false;
        }

        let enum_members = union_members.iter().collect::<Vec<_>>();
        Self::push_string_enum_members_to_formatter(enum_members.as_slice(), formatter);

        true
    }

    fn push_string_enum_members_to_formatter(enum_members: &[&Self], formatter: &mut DslFormatter) {
        formatter.output.push_str("enum { ");

        let mut enum_member_iterator = enum_members.iter().peekable();

        while let Some(enum_member) = enum_member_iterator.next() {
            if let Self::StringEnum(enum_value) = enum_member {
                formatter.output.push_str(enum_value);
            }

            if enum_member_iterator.peek().is_some() {
                formatter.output.push_str(", ");
            }
        }

        formatter.output.push_str(" }");
    }

    fn should_break_inside_array(&self) -> bool {
        match self {
            Self::Object(_) | Self::Variant { discriminator: _, cases: _ } => true,
            Self::Array { item_type, fixed_length: _ } => item_type.should_break_inside_array(),
            Self::Tuple(tuple_items) | Self::Union(tuple_items) => tuple_items.iter().any(Self::should_break_inside_array),
            Self::String
            | Self::Number
            | Self::Float
            | Self::Boolean
            | Self::Null
            | Self::AnyObject
            | Self::SchemaReference(_)
            | Self::StringEnum(_)
            | Self::StringEnumReference(_) => false,
        }
    }
}
