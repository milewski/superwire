use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use clap::{Args, Subcommand};

use crate::diagnostics::CommandError;

const SAMPLE_TOOL_WIT_SOURCE: &str = r"package superwire:sample-tool@0.1.0;

interface types {
    record create-task-request {
        task-group-id: u64,
    }

    record workspace-context {
        workspace-id: u64,
        project-id: u64,
    }

    record create-task-result {
        task-id: u64,
        number: u64,
    }

    type input = create-task-request;
    type bounded-input = workspace-context;
    type output = create-task-result;
}

interface tool {
    use types.{input, bounded-input, output};

    execute: func(input: input, bounded-input: bounded-input) -> result<output, string>;
}
";

#[derive(Debug, Args)]
pub struct ToolCommand {
    #[command(subcommand)]
    command: ToolSubcommand,
}

impl ToolCommand {
    pub fn execute(self) -> Result<(), CommandError> {
        match self.command {
            ToolSubcommand::Init(init_tool_command) => init_tool_command.execute(),
            ToolSubcommand::Prepare(prepare_tool_command) => prepare_tool_command.execute(),
        }
    }
}

#[derive(Debug, Subcommand)]
enum ToolSubcommand {
    Init(InitToolCommand),
    Prepare(PrepareToolCommand),
}

#[derive(Debug, Args)]
struct InitToolCommand {
    #[arg(value_name = "DIRECTORY", default_value = ".")]
    directory: PathBuf,
}

impl InitToolCommand {
    fn execute(self) -> Result<(), CommandError> {
        let output_directory = if self.directory.is_absolute() {
            self.directory
        } else {
            Path::new(".").join(self.directory)
        };

        fs::create_dir_all(&output_directory)
            .map_err(|error| CommandError::internal(format!("failed to create tool directory {}: {error}", output_directory.display())))?;

        let sample_wit_path = output_directory.join("sample-tool.wit");

        if sample_wit_path.exists() {
            return Err(CommandError::invalid_input(format!(
                "sample file already exists: {}",
                sample_wit_path.display()
            )));
        }

        fs::write(&sample_wit_path, SAMPLE_TOOL_WIT_SOURCE)
            .map_err(|error| CommandError::internal(format!("failed to write sample WIT file {}: {error}", sample_wit_path.display())))?;

        println!("initialized {}", sample_wit_path.display());
        println!("next: cli tool prepare {}", output_directory.display());

        Ok(())
    }
}

#[derive(Debug, Args)]
struct PrepareToolCommand {
    #[arg(value_name = "DIRECTORY", default_value = ".")]
    directory: PathBuf,

    #[arg(long, value_name = "LANGUAGE", default_value = "php")]
    language: String,
}

impl PrepareToolCommand {
    fn execute(self) -> Result<(), CommandError> {
        let target_language = TargetLanguage::from_identifier(&self.language)
            .ok_or_else(|| CommandError::invalid_input(format!("unsupported language `{}`. supported languages: php", self.language)))?;

        let canonical_directory = fs::canonicalize(&self.directory)
            .map_err(|_| CommandError::invalid_input(format!("directory does not exist: {}", self.directory.display())))?;

        if !canonical_directory.is_dir() {
            return Err(CommandError::invalid_input(format!(
                "path is not a directory: {}",
                canonical_directory.display()
            )));
        }

        let tool_specifications = ToolSpecification::collect_from_directory(&canonical_directory)?;

        if tool_specifications.is_empty() {
            return Err(CommandError::invalid_input(format!(
                "no tool type files found in {}",
                canonical_directory.display()
            )));
        }

        let output_directory = canonical_directory.join(target_language.as_str());

        fs::create_dir_all(&output_directory).map_err(|error| {
            CommandError::internal(format!("failed to create output directory {}: {error}", output_directory.display()))
        })?;

        for tool_specification in &tool_specifications {
            tool_specification.write_scaffold(&output_directory, target_language)?;
        }

        println!(
            "generated {} tool scaffold(s) in {}",
            tool_specifications.len(),
            output_directory.display()
        );

        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TargetLanguage {
    Php,
}

impl TargetLanguage {
    fn from_identifier(identifier: &str) -> Option<Self> {
        match identifier {
            "php" => Some(Self::Php),
            _ => None,
        }
    }

    const fn as_str(self) -> &'static str {
        match self {
            Self::Php => "php",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ToolSpecification {
    tool_name: String,
    records: Vec<WitRecord>,
    record_name_by_contract_type: HashMap<ToolContractTypeName, String>,
}

impl ToolSpecification {
    fn collect_from_directory(tool_directory: &Path) -> Result<Vec<Self>, CommandError> {
        let mut tool_specifications = Vec::new();

        for directory_entry_result in fs::read_dir(tool_directory)
            .map_err(|error| CommandError::internal(format!("failed to read directory {}: {error}", tool_directory.display())))?
        {
            let directory_entry =
                directory_entry_result.map_err(|error| CommandError::internal(format!("failed to read directory entry: {error}")))?;
            let entry_path = directory_entry.path();

            if !entry_path.is_file() {
                continue;
            }

            if entry_path.extension().and_then(|extension| extension.to_str()) != Some("wit") {
                continue;
            }

            let Some(file_stem) = entry_path.file_stem().and_then(|name| name.to_str()) else {
                continue;
            };

            if file_stem == "superwire-tool" {
                continue;
            }

            let source = fs::read_to_string(&entry_path)
                .map_err(|error| CommandError::internal(format!("failed to read WIT file {}: {error}", entry_path.display())))?;

            let wit_source = WitSource::from_source(&source);
            let parsed_records = wit_source
                .parse_types_records()
                .map_err(|message| CommandError::invalid_input(format!("{} ({})", message, entry_path.display())))?;
            let parsed_contract = wit_source
                .parse_tool_contract()
                .map_err(|message| CommandError::invalid_input(format!("{} ({})", message, entry_path.display())))?;

            tool_specifications.push(Self::from_records_and_contract(
                file_stem.to_string(),
                parsed_records,
                parsed_contract,
            )?);
        }

        tool_specifications
            .sort_by(|first_specification, second_specification| first_specification.tool_name.cmp(&second_specification.tool_name));

        Ok(tool_specifications)
    }

    fn from_records_and_contract(tool_name: String, records: Vec<WitRecord>, tool_contract: ToolContract) -> Result<Self, CommandError> {
        let mut records_by_name = HashSet::<String>::new();

        for record in &records {
            if !records_by_name.insert(record.name.clone()) {
                return Err(CommandError::invalid_input(format!(
                    "`{tool_name}.wit` contains duplicate record `{}` in `interface types`",
                    record.name
                )));
            }
        }

        let mut record_name_by_contract_type = HashMap::<ToolContractTypeName, String>::new();

        for contract_alias in ToolContractTypeName::all() {
            let Some(aliased_record_name) = tool_contract.alias_target_by_type.get(&contract_alias) else {
                return Err(CommandError::invalid_input(format!(
                    "`{tool_name}.wit` is missing `type {} = ...;` in `interface types`",
                    contract_alias.as_str()
                )));
            };

            if !records_by_name.contains(aliased_record_name) {
                return Err(CommandError::invalid_input(format!(
                    "`{tool_name}.wit` maps `{}` to `{aliased_record_name}`, but that record is not defined in `interface types`",
                    contract_alias.as_str()
                )));
            }

            record_name_by_contract_type.insert(contract_alias, aliased_record_name.clone());
        }

        Ok(Self {
            tool_name,
            records,
            record_name_by_contract_type,
        })
    }

    fn class_name_for_record(&self, contract_type_name: ToolContractTypeName) -> String {
        let aliased_record_name = self
            .record_name_by_contract_type
            .get(&contract_type_name)
            .expect("required aliased record name should exist");

        self.records
            .iter()
            .find(|record| record.name == *aliased_record_name)
            .map(WitRecord::class_name)
            .expect("required tool record should exist")
    }

    fn write_scaffold(&self, output_directory: &Path, target_language: TargetLanguage) -> Result<(), CommandError> {
        let tool_output_directory = output_directory.join(&self.tool_name);

        fs::create_dir_all(&tool_output_directory).map_err(|error| {
            CommandError::internal(format!(
                "failed to create output directory {}: {error}",
                tool_output_directory.display()
            ))
        })?;

        match target_language {
            TargetLanguage::Php => self.write_php_scaffold(&tool_output_directory),
        }
    }

    fn write_php_scaffold(&self, output_directory: &Path) -> Result<(), CommandError> {
        let source_renderer = PhpSourceRenderer::from_tool_specification(self);

        for wit_record in &self.records {
            let file_name = source_renderer.record_file_name(wit_record);
            let record_source = source_renderer.render_record_source(wit_record);
            let destination_path = output_directory.join(file_name);

            fs::write(&destination_path, record_source)
                .map_err(|error| CommandError::internal(format!("failed to write scaffold {}: {error}", destination_path.display())))?;

            println!("generated {}", destination_path.display());
        }

        let tool_file_name = source_renderer.tool_file_name();
        let tool_source = source_renderer.render_tool_source();
        let tool_destination_path = output_directory.join(tool_file_name);

        fs::write(&tool_destination_path, tool_source)
            .map_err(|error| CommandError::internal(format!("failed to write scaffold {}: {error}", tool_destination_path.display())))?;

        println!("generated {}", tool_destination_path.display());

        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum ToolContractTypeName {
    Input,
    BoundedInput,
    Output,
}

impl ToolContractTypeName {
    fn from_identifier(identifier: &str) -> Option<Self> {
        match identifier {
            "input" => Some(Self::Input),
            "bounded-input" => Some(Self::BoundedInput),
            "output" => Some(Self::Output),
            _ => None,
        }
    }

    const fn all() -> [Self; 3] {
        [Self::Input, Self::BoundedInput, Self::Output]
    }

    const fn as_str(self) -> &'static str {
        match self {
            Self::Input => "input",
            Self::BoundedInput => "bounded-input",
            Self::Output => "output",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ToolContract {
    alias_target_by_type: HashMap<ToolContractTypeName, String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ParsedTypesInterface {
    records: Vec<WitRecord>,
    alias_target_by_type: HashMap<ToolContractTypeName, String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ParsedExecuteSignature {
    input: String,
    bounded_input: String,
    output: String,
    error: String,
}

struct WitSource<'source> {
    source: &'source str,
}

impl<'source> WitSource<'source> {
    fn from_source(source: &'source str) -> Self {
        Self { source }
    }

    fn parse_types_records(&self) -> Result<Vec<WitRecord>, String> {
        let types_interface_body = self.interface_body("types")?;
        let parsed_records_and_aliases = Self::parse_types_records_and_aliases(types_interface_body)?;

        Ok(parsed_records_and_aliases.records)
    }

    fn parse_tool_contract(&self) -> Result<ToolContract, String> {
        let types_interface_body = self.interface_body("types")?;
        let parsed_records_and_aliases = Self::parse_types_records_and_aliases(types_interface_body)?;
        let tool_interface_body = self.interface_body("tool")?;
        let execute_signature = Self::parse_execute_signature(tool_interface_body)?;

        if execute_signature.input != ToolContractTypeName::Input.as_str() {
            return Err("`interface tool` execute first parameter type must be `input`".to_string());
        }

        if execute_signature.bounded_input != ToolContractTypeName::BoundedInput.as_str() {
            return Err("`interface tool` execute second parameter type must be `bounded-input`".to_string());
        }

        if execute_signature.output != ToolContractTypeName::Output.as_str() {
            return Err("`interface tool` execute success type must be `output`".to_string());
        }

        if execute_signature.error != "tool-error" && execute_signature.error != "string" {
            return Err("`interface tool` execute error type must be `string` or `tool-error`".to_string());
        }

        for required_alias in ToolContractTypeName::all() {
            if !parsed_records_and_aliases.alias_target_by_type.contains_key(&required_alias) {
                return Err(format!("missing `type {} = ...;` in `interface types`", required_alias.as_str()));
            }
        }

        Ok(ToolContract {
            alias_target_by_type: parsed_records_and_aliases.alias_target_by_type,
        })
    }

    fn interface_body(&self, interface_name: &str) -> Result<&'source str, String> {
        let interface_header = format!("interface {interface_name}");
        let interface_start = self
            .source
            .find(interface_header.as_str())
            .ok_or_else(|| format!("missing `interface {interface_name}` block"))?;
        let interface_block_start = self
            .source
            .get(interface_start..)
            .and_then(|source_slice| source_slice.find('{').map(|open_brace_index| open_brace_index + interface_start))
            .ok_or_else(|| format!("missing `{{` for `interface {interface_name}` block"))?;
        let interface_block_end = self
            .matching_brace_index(interface_block_start)
            .ok_or_else(|| format!("missing closing `}}` for `interface {interface_name}` block"))?;

        Ok(&self.source[(interface_block_start + 1)..interface_block_end])
    }

    fn parse_types_records_and_aliases(types_interface_body: &str) -> Result<ParsedTypesInterface, String> {
        let mut records = Vec::new();
        let mut alias_target_by_type = HashMap::<ToolContractTypeName, String>::new();
        let mut scan_offset = 0;
        let mut parsed_record_ranges = Vec::<(usize, usize)>::new();

        while let Some(record_start_offset) = types_interface_body[scan_offset..].find("record ") {
            let record_keyword_index = scan_offset + record_start_offset;
            let record_name_start = record_keyword_index + "record ".len();

            let record_body_open_index = types_interface_body[record_name_start..]
                .find('{')
                .map(|open_brace_index| open_brace_index + record_name_start)
                .ok_or_else(|| "missing `{` for record".to_string())?;

            let raw_record_name = types_interface_body[record_name_start..record_body_open_index].trim();

            if raw_record_name.is_empty() {
                return Err("record name cannot be empty".to_string());
            }

            let record_body_end_index = Self::matching_brace_index_for(types_interface_body, record_body_open_index)
                .ok_or_else(|| format!("missing `}}` for record `{raw_record_name}`"))?;

            let record_body = &types_interface_body[(record_body_open_index + 1)..record_body_end_index];
            let parsed_fields = Self::parse_record_fields(record_body)?;

            records.push(WitRecord {
                name: raw_record_name.to_string(),
                fields: parsed_fields,
            });

            parsed_record_ranges.push((record_keyword_index, record_body_end_index + 1));

            scan_offset = record_body_end_index + 1;
        }

        let mut non_record_content = String::new();
        let mut previous_range_end = 0;

        for (range_start, range_end) in parsed_record_ranges {
            non_record_content.push_str(&types_interface_body[previous_range_end..range_start]);
            previous_range_end = range_end;
        }

        non_record_content.push_str(&types_interface_body[previous_range_end..]);

        for source_line in non_record_content.lines() {
            let trimmed_line = source_line.trim();

            if trimmed_line.is_empty() || trimmed_line.starts_with("///") {
                continue;
            }

            let Some(type_alias_body) = trimmed_line.strip_prefix("type ") else {
                return Err("`interface types` can only contain record and type declarations".to_string());
            };

            let type_alias_body = type_alias_body.trim_end_matches(';').trim();
            let Some((alias_name, target_type_name)) = type_alias_body.split_once('=') else {
                return Err(format!("invalid type alias declaration: `{trimmed_line}`"));
            };

            let alias_name = alias_name.trim();
            let target_type_name = target_type_name.trim();
            let Some(contract_type_name) = ToolContractTypeName::from_identifier(alias_name) else {
                return Err(format!(
                    "unsupported type alias `{alias_name}` in `interface types`. allowed aliases: input, bounded-input, output"
                ));
            };

            if target_type_name.is_empty() {
                return Err(format!("invalid type alias declaration: `{trimmed_line}`"));
            }

            if alias_target_by_type
                .insert(contract_type_name, target_type_name.to_string())
                .is_some()
            {
                return Err(format!("duplicate type alias `{alias_name}` in `interface types`"));
            }
        }

        Ok(ParsedTypesInterface {
            records,
            alias_target_by_type,
        })
    }

    fn parse_execute_signature(tool_interface_body: &str) -> Result<ParsedExecuteSignature, String> {
        let mut execute_signature: Option<ParsedExecuteSignature> = None;

        for statement in tool_interface_body.split(';') {
            let trimmed_statement = statement.trim();

            if trimmed_statement.is_empty() || trimmed_statement.starts_with("///") {
                continue;
            }

            if trimmed_statement.starts_with("use ") {
                continue;
            }

            let Some(execute_body) = trimmed_statement.strip_prefix("execute:") else {
                return Err("`interface tool` can only contain `use` declarations and `execute`".to_string());
            };

            if execute_signature.is_some() {
                return Err("`interface tool` must define exactly one `execute` function".to_string());
            }

            execute_signature = Some(Self::parse_execute_body(execute_body.trim())?);
        }

        execute_signature.ok_or_else(|| "missing `execute` function in `interface tool`".to_string())
    }

    fn parse_execute_body(execute_body: &str) -> Result<ParsedExecuteSignature, String> {
        let execute_body = execute_body.trim();
        let Some(parameters_start_index) = execute_body.find("func(") else {
            return Err("`interface tool` execute must use `func(...)`".to_string());
        };
        let parameters_source_start = parameters_start_index + "func(".len();
        let parameters_source_end = execute_body[parameters_source_start..]
            .find(')')
            .map(|closing_parenthesis_index| parameters_source_start + closing_parenthesis_index)
            .ok_or_else(|| "`interface tool` execute has invalid parameter list".to_string())?;
        let parameters_source = &execute_body[parameters_source_start..parameters_source_end];
        let parameter_entries = parameters_source
            .split(',')
            .map(str::trim)
            .filter(|parameter_entry| !parameter_entry.is_empty())
            .collect::<Vec<_>>();

        if parameter_entries.len() != 2 {
            return Err("`interface tool` execute must accept exactly two parameters".to_string());
        }

        let input_type = Self::parse_parameter_type_name(parameter_entries[0])?;
        let bounded_input_type = Self::parse_parameter_type_name(parameter_entries[1])?;
        let result_source = execute_body[(parameters_source_end + 1)..].trim();
        let result_source = result_source
            .strip_prefix("->")
            .ok_or_else(|| "`interface tool` execute must return `result<output, string>` or `result<output, tool-error>`".to_string())?
            .trim();
        let Some(result_inner_source) = result_source.strip_prefix("result<").and_then(|source| source.strip_suffix('>')) else {
            return Err("`interface tool` execute must return `result<output, string>` or `result<output, tool-error>`".to_string());
        };
        let result_type_entries = result_inner_source
            .split(',')
            .map(str::trim)
            .filter(|entry| !entry.is_empty())
            .collect::<Vec<_>>();

        if result_type_entries.len() != 2 {
            return Err("`interface tool` execute must return `result<output, string>` or `result<output, tool-error>`".to_string());
        }

        Ok(ParsedExecuteSignature {
            input: input_type.to_string(),
            bounded_input: bounded_input_type.to_string(),
            output: result_type_entries[0].to_string(),
            error: result_type_entries[1].to_string(),
        })
    }

    fn parse_parameter_type_name(parameter_entry: &str) -> Result<&str, String> {
        let Some((_, parameter_type_name)) = parameter_entry.split_once(':') else {
            return Err("`interface tool` execute has invalid parameter".to_string());
        };

        let parameter_type_name = parameter_type_name.trim();

        if parameter_type_name.is_empty() {
            return Err("`interface tool` execute has invalid parameter".to_string());
        }

        Ok(parameter_type_name)
    }

    fn parse_record_fields(record_body: &str) -> Result<Vec<WitField>, String> {
        let mut parsed_fields = Vec::new();

        for line in record_body.lines() {
            let trimmed_line = line.trim();

            if trimmed_line.is_empty() || trimmed_line.starts_with("///") {
                continue;
            }

            let line_without_trailing_comma = trimmed_line.trim_end_matches(',').trim();

            if line_without_trailing_comma.is_empty() {
                continue;
            }

            let Some((raw_field_name, raw_field_type)) = line_without_trailing_comma.split_once(':') else {
                return Err(format!("invalid field line: `{line_without_trailing_comma}`"));
            };

            let field_name = raw_field_name.trim();
            let field_type = raw_field_type.trim();

            if field_name.is_empty() || field_type.is_empty() {
                return Err(format!("invalid field line: `{line_without_trailing_comma}`"));
            }

            parsed_fields.push(WitField {
                name: field_name.to_string(),
                field_type: WitType::from_source(field_type)?,
            });
        }

        Ok(parsed_fields)
    }

    fn matching_brace_index(&self, open_brace_index: usize) -> Option<usize> {
        Self::matching_brace_index_for(self.source, open_brace_index)
    }

    fn matching_brace_index_for(source: &str, open_brace_index: usize) -> Option<usize> {
        let mut brace_depth = 0_usize;

        for (byte_index, character) in source.char_indices().skip(open_brace_index) {
            if character == '{' {
                brace_depth += 1;

                continue;
            }

            if character == '}' {
                brace_depth = brace_depth.checked_sub(1)?;

                if brace_depth == 0 {
                    return Some(byte_index);
                }
            }
        }

        None
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct WitRecord {
    name: String,
    fields: Vec<WitField>,
}

impl WitRecord {
    fn class_name(&self) -> String {
        Self::to_pascal_case(&self.name)
    }

    fn file_name(&self) -> String {
        format!("{}.php", self.class_name())
    }

    fn to_pascal_case(raw_identifier: &str) -> String {
        let mut pascal_case_name = String::new();

        for raw_segment in raw_identifier
            .split(|character: char| character == '-' || character == '_' || character.is_whitespace())
            .filter(|segment| !segment.is_empty())
        {
            let mut segment_characters = raw_segment.chars();

            if let Some(first_character) = segment_characters.next() {
                pascal_case_name.push(first_character.to_ascii_uppercase());
                pascal_case_name.extend(segment_characters);
            }
        }

        pascal_case_name
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct WitField {
    name: String,
    field_type: WitType,
}

impl WitField {
    fn property_name(&self) -> String {
        let mut property_name = String::new();
        let mut should_uppercase_next_character = false;

        for character in self.name.chars() {
            if character == '-' || character == '_' {
                should_uppercase_next_character = true;

                continue;
            }

            if property_name.is_empty() {
                property_name.push(character.to_ascii_lowercase());
                should_uppercase_next_character = false;

                continue;
            }

            if should_uppercase_next_character {
                property_name.push(character.to_ascii_uppercase());
                should_uppercase_next_character = false;
            } else {
                property_name.push(character);
            }
        }

        property_name
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum WitType {
    Bool,
    String,
    U32,
    U64,
    S32,
    S64,
    F32,
    F64,
    Option(Box<WitType>),
    List(Box<WitType>),
    RecordReference(String),
}

impl WitType {
    fn from_source(source: &str) -> Result<Self, String> {
        let trimmed_source = source.trim();

        if let Some(inner_type) = trimmed_source
            .strip_prefix("option<")
            .and_then(|without_prefix| without_prefix.strip_suffix('>'))
        {
            return Ok(Self::Option(Box::new(Self::from_source(inner_type)?)));
        }

        if let Some(inner_type) = trimmed_source
            .strip_prefix("list<")
            .and_then(|without_prefix| without_prefix.strip_suffix('>'))
        {
            return Ok(Self::List(Box::new(Self::from_source(inner_type)?)));
        }

        match trimmed_source {
            "bool" => Ok(Self::Bool),
            "string" => Ok(Self::String),
            "u32" => Ok(Self::U32),
            "u64" => Ok(Self::U64),
            "s32" => Ok(Self::S32),
            "s64" => Ok(Self::S64),
            "f32" => Ok(Self::F32),
            "f64" => Ok(Self::F64),
            _ => Ok(Self::RecordReference(trimmed_source.to_string())),
        }
    }
}

struct PhpSourceRenderer<'specification> {
    tool_specification: &'specification ToolSpecification,
}

impl<'specification> PhpSourceRenderer<'specification> {
    fn from_tool_specification(tool_specification: &'specification ToolSpecification) -> Self {
        Self { tool_specification }
    }

    fn namespace_name(&self) -> String {
        format!(
            "Superwire\\Generated\\{}",
            WitRecord::to_pascal_case(&self.tool_specification.tool_name)
        )
    }

    fn tool_class_name(&self) -> String {
        format!("{}Tool", WitRecord::to_pascal_case(&self.tool_specification.tool_name))
    }

    fn tool_file_name(&self) -> String {
        format!("{}.php", self.tool_class_name())
    }

    fn record_file_name(&self, wit_record: &WitRecord) -> String {
        let _ = self;

        wit_record.file_name()
    }

    fn render_record_source(&self, wit_record: &WitRecord) -> String {
        let mut rendered_lines = Vec::new();

        rendered_lines.push("<?php".to_string());
        rendered_lines.push("declare(strict_types=1);".to_string());
        rendered_lines.push(String::new());
        rendered_lines.push(format!("namespace {};", self.namespace_name()));
        rendered_lines.push(String::new());
        rendered_lines.push(format!("final class {}", wit_record.class_name()));
        rendered_lines.push("{".to_string());
        rendered_lines.push("    public function __construct(".to_string());

        for (field_index, wit_field) in wit_record.fields.iter().enumerate() {
            let rendered_property_type = Self::render_property_type(&wit_field.field_type);
            let rendered_separator = if field_index + 1 == wit_record.fields.len() { "" } else { "," };

            rendered_lines.push(format!(
                "        public {} ${}{}",
                rendered_property_type,
                wit_field.property_name(),
                rendered_separator
            ));
        }

        rendered_lines.push("    ) {".to_string());
        rendered_lines.push("    }".to_string());
        rendered_lines.push("}".to_string());

        rendered_lines.join("\n")
    }

    fn render_tool_source(&self) -> String {
        let input_type = self.tool_specification.class_name_for_record(ToolContractTypeName::Input);
        let output_type = self.tool_specification.class_name_for_record(ToolContractTypeName::Output);
        let bounded_input_type = self.tool_specification.class_name_for_record(ToolContractTypeName::BoundedInput);
        let bound_type = format!("?{bounded_input_type}");
        let mut rendered_lines = Vec::new();

        rendered_lines.push("<?php".to_string());
        rendered_lines.push("declare(strict_types=1);".to_string());
        rendered_lines.push(String::new());
        rendered_lines.push(format!("namespace {};", self.namespace_name()));
        rendered_lines.push(String::new());
        rendered_lines.push(format!("class {} extends \\Superwire\\Tool", self.tool_class_name()));
        rendered_lines.push("{".to_string());
        rendered_lines.push(format!(
            "    public function execute({input_type} $input, {bound_type} $boundInput): {output_type}"
        ));
        rendered_lines.push("    {".to_string());
        rendered_lines.push(format!("        return new {output_type}("));
        rendered_lines.push("            // ...".to_string());
        rendered_lines.push("        );".to_string());
        rendered_lines.push("    }".to_string());
        rendered_lines.push("}".to_string());

        rendered_lines.join("\n")
    }

    fn render_property_type(wit_type: &WitType) -> String {
        match wit_type {
            WitType::Bool => "bool".to_string(),
            WitType::String => "string".to_string(),
            WitType::U32 | WitType::U64 | WitType::S32 | WitType::S64 => "int".to_string(),
            WitType::F32 | WitType::F64 => "float".to_string(),
            WitType::Option(inner_type) => {
                let inner_property_type = Self::render_property_type(inner_type);

                if inner_property_type.starts_with('?') {
                    inner_property_type
                } else {
                    format!("?{inner_property_type}")
                }
            }
            WitType::List(_) => "array".to_string(),
            WitType::RecordReference(record_name) => WitRecord::to_pascal_case(record_name),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::{PhpSourceRenderer, ToolContract, ToolContractTypeName, ToolSpecification, WitField, WitRecord, WitSource, WitType};

    #[test]
    fn parses_wit_records_from_types_interface() {
        let wit_source = WitSource::from_source(
            "package superwire:test@0.1.0;

            interface types {
                record create-task-request {
                    task-group-id: u64,
                    tags: list<string>,
                }

                record workspace-context {
                    workspace-id: u64,
                    token: option<string>,
                }

                record create-task-result {
                    success: bool,
                }

                type input = create-task-request;
                type bounded-input = workspace-context;
                type output = create-task-result;
            }

            interface tool {
                use types.{input, bounded-input, output};
                use superwire:tool@0.1.0/types.{tool-error};

                execute: func(request: input, context: bounded-input) -> result<output, tool-error>;
            }",
        );

        let parsed_records = wit_source.parse_types_records().expect("types should parse");
        let parsed_contract = wit_source.parse_tool_contract().expect("contract should parse");

        assert_eq!(parsed_records.len(), 3);
        assert_eq!(parsed_records[0].name, "create-task-request");
        assert_eq!(parsed_records[1].name, "workspace-context");
        assert_eq!(parsed_records[2].name, "create-task-result");
        assert_eq!(
            parsed_contract
                .alias_target_by_type
                .get(&ToolContractTypeName::Input)
                .expect("input alias should exist"),
            "create-task-request"
        );
    }

    #[test]
    fn renders_php_source_with_separate_files() {
        let records = vec![
            WitRecord {
                name: "create-task-request".to_string(),
                fields: vec![WitField {
                    name: "task-group-id".to_string(),
                    field_type: WitType::U64,
                }],
            },
            WitRecord {
                name: "workspace-context".to_string(),
                fields: vec![WitField {
                    name: "workspace-id".to_string(),
                    field_type: WitType::U64,
                }],
            },
            WitRecord {
                name: "create-task-result".to_string(),
                fields: vec![WitField {
                    name: "task-id".to_string(),
                    field_type: WitType::U64,
                }],
            },
        ];
        let contract = ToolContract {
            alias_target_by_type: HashMap::from([
                (ToolContractTypeName::Input, "create-task-request".to_string()),
                (ToolContractTypeName::BoundedInput, "workspace-context".to_string()),
                (ToolContractTypeName::Output, "create-task-result".to_string()),
            ]),
        };
        let tool_specification =
            ToolSpecification::from_records_and_contract("tool-a".to_string(), records, contract).expect("tool specification should build");

        let source_renderer = PhpSourceRenderer::from_tool_specification(&tool_specification);
        let input_source = source_renderer.render_record_source(&tool_specification.records[0]);
        let tool_source = source_renderer.render_tool_source();

        assert_eq!(
            source_renderer.record_file_name(&tool_specification.records[0]),
            "CreateTaskRequest.php"
        );
        assert_eq!(source_renderer.tool_file_name(), "ToolATool.php");
        assert!(input_source.contains("final class CreateTaskRequest"));
        assert!(input_source.contains("public int $taskGroupId"));
        assert!(tool_source.contains("class ToolATool extends \\Superwire\\Tool"));
        assert!(tool_source.contains("public function execute(CreateTaskRequest $input, ?WorkspaceContext $boundInput): CreateTaskResult"));
        assert!(tool_source.contains("return new CreateTaskResult("));
    }

    #[test]
    fn rejects_execute_function_inside_types_interface() {
        let wit_source = WitSource::from_source(
            "package superwire:test@0.1.0;

            interface types {
                record input {
                    task-group-id: u64,
                }

                record bounded-input {
                    workspace-id: u64,
                }

                record output {
                    task-id: u64,
                }

                execute: func(input: input, bounded-input: bounded-input) -> result<string, string>;
            }

            interface tool {
                execute: func(input: input, bounded-input: bounded-input) -> result<output, tool-error>;
            }",
        );

        let parse_error = wit_source
            .parse_types_records()
            .expect_err("types interface with function should fail");

        assert!(parse_error.contains("record and type declarations"));
    }

    #[test]
    fn rejects_invalid_tool_execute_contract() {
        let wit_source = WitSource::from_source(
            "package superwire:test@0.1.0;

            interface types {
                record request {
                    id: u64,
                }

                record context {
                    workspace-id: u64,
                }

                record result {
                    ok: bool,
                }

                type input = request;
                type bounded-input = context;
                type output = result;
            }

            interface tool {
                execute: func(input: input, bounded-input: bounded-input) -> result<output, u64>;
            }",
        );

        let parse_error = wit_source
            .parse_tool_contract()
            .expect_err("unsupported execute error type should fail");

        assert!(parse_error.contains("error type must be `string` or `tool-error`"));
    }
}
