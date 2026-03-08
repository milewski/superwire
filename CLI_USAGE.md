# CLI Usage Guide

## Running Workflows

The `engine-ai-example` binary can execute `.ai` workflow files with various input options.

## Basic Usage

```bash
engine-ai-example <workflow.ai> [OPTIONS]
```

## Input Options

### 1. Using `--value` for inline values

Pass input values directly via command line arguments:

```bash
# Single value
engine-ai-example workflow.ai --value name=Alice

# Multiple values
engine-ai-example workflow.ai --value name=Alice --value age=30

# String values with spaces (use quotes)
engine-ai-example workflow.ai --value topic="quantum computing"

# Boolean values
engine-ai-example workflow.ai --value enabled=true

# Numeric values
engine-ai-example workflow.ai --value count=42 --value price=19.99

# Null values
engine-ai-example workflow.ai --value optional=null
```

### 2. Using `--input` for JSON files

Load all inputs from a JSON file:

```bash
engine-ai-example workflow.ai --input inputs.json
```

Where `inputs.json` contains:
```json
{
  "name": "Alice",
  "age": 30,
  "topic": "quantum computing"
}
```

### 3. Combining both methods

You can combine `--input` and `--value`. Values from `--value` will override values from the file:

```bash
engine-ai-example workflow.ai --input inputs.json --value name=Bob
```

## Value Type Parsing

The CLI automatically parses values to the appropriate JSON type:

| Input | Parsed As | JSON Type |
|-------|-----------|-----------|
| `name=Alice` | `"Alice"` | String |
| `age=30` | `30` | Number |
| `price=19.99` | `19.99` | Number |
| `enabled=true` | `true` | Boolean |
| `disabled=false` | `false` | Boolean |
| `optional=null` | `null` | Null |
| `tags=[1,2,3]` | `[1,2,3]` | Array |
| `config={"key":"value"}` | `{"key":"value"}` | Object |

## Examples

### Example 1: Simple greeting

```bash
./target/release/engine-ai-example \
  ./crates/example/workflows/terminal_with_output.ai \
  --value user_name=Alice
```

Output:
```json
{
  "greeting": "Hello, Alice! 😊",
  "timestamp": "2026-03-08",
  "user": "Alice"
}
```

### Example 2: Research workflow

```bash
./target/release/engine-ai-example \
  ./crates/example/workflows/input_output.ai \
  --value topic="artificial intelligence" \
  --value audience="high school students"
```

### Example 3: Using JSON file

Create `inputs.json`:
```json
{
  "topic": "quantum computing",
  "audience": "university students"
}
```

Run:
```bash
./target/release/engine-ai-example \
  ./crates/example/workflows/input_output.ai \
  --input inputs.json
```

### Example 4: Override file values

```bash
./target/release/engine-ai-example \
  ./crates/example/workflows/input_output.ai \
  --input inputs.json \
  --value audience="professionals"
```

## Using with cargo run

During development, you can use `cargo run`:

```bash
# With --value
cargo run --release --bin engine-ai-example -- \
  ./crates/example/workflows/terminal_with_output.ai \
  --value user_name=Alice

# With --input
cargo run --release --bin engine-ai-example -- \
  ./crates/example/workflows/input_output.ai \
  --input ./crates/example/workflows/input_output_inputs.json
```

Note: The `--` separates cargo arguments from program arguments.

## Error Handling

### Missing required input

If a workflow requires an input that isn't provided:

```bash
./target/release/engine-ai-example workflow.ai
```

Error:
```
Error: Runtime error in agent input: Input field 'name' not found
```

### Invalid value format

If using `--value` without `=`:

```bash
./target/release/engine-ai-example workflow.ai --value name
```

Error:
```
Error: --value argument must be in format key=value
```

### Invalid JSON file

If the JSON file is malformed:

```bash
./target/release/engine-ai-example workflow.ai --input bad.json
```

Error:
```
Failed to parse input JSON: expected value at line 1 column 1
```

## Tips

1. **Quote strings with spaces**: Always use quotes for values containing spaces
   ```bash
   --value message="Hello World"
   ```

2. **Escape special characters**: Use shell escaping for special characters
   ```bash
   --value path="/home/user/file.txt"
   ```

3. **Multiple values**: You can use `--value` multiple times
   ```bash
   --value a=1 --value b=2 --value c=3
   ```

4. **JSON values**: For complex values, use JSON syntax
   ```bash
   --value config='{"timeout":30,"retries":3}'
   ```

5. **Check workflow inputs**: Look at the `input` block in the `.ai` file to see what inputs are required
   ```ai
   input {
       name: string
       age: number
   }
   ```
