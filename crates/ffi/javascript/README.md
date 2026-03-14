# Engine AI JavaScript/TypeScript Bindings

JavaScript and TypeScript bindings for Engine AI, providing a native Node.js interface to the Engine AI workflow execution engine.

## Installation

```bash
npm install @engine-ai/javascript
```

## Features

- **Native Performance**: Built with Rust and NAPI-RS for optimal performance
- **TypeScript Support**: Full TypeScript definitions included
- **Zod Integration**: Easy schema definition using Zod for type-safe tool creation
- **Async/Await**: Modern async API for workflow execution
- **Custom Tools**: Create custom tools with JavaScript/TypeScript functions

## Quick Start

### Basic Usage

```typescript
import { ExecutionEngine } from '@engine-ai/javascript';

const engine = new ExecutionEngine();

const workflow = `
  provider ollama {
    driver <- "ollama"
    models <- ["qwen3:8b"]
    config <- { endpoint <- "http://100.76.5.36:11434" }
  }
  
  agent poet {
    model <- "ollama/qwen3:8b"
    output <- string
    prompt <- "Write a haiku about programming"
  }
`;

const result = await engine.executeWorkflowContent(workflow);
console.log(result);
```

### Creating Custom Tools with Zod

#### Method 1: Using createTool Helper

```typescript
import { ExecutionEngine } from '@engine-ai/javascript';
import { createTool } from '@engine-ai/javascript/tool-helper';
import { z } from 'zod';

const calculatorTool = createTool(
  'calculator',
  'Perform arithmetic operations',
  z.object({
    operation: z.enum(['add', 'subtract', 'multiply', 'divide']),
    a: z.number(),
    b: z.number()
  }),
  (params) => {
    // params is fully typed!
    switch (params.operation) {
      case 'add': return { result: params.a + params.b };
      case 'subtract': return { result: params.a - params.b };
      case 'multiply': return { result: params.a * params.b };
      case 'divide': return { result: params.a / params.b };
    }
  }
);

const engine = ExecutionEngine.withTools([calculatorTool]);
```

#### Method 2: Using ZodTool Class

```typescript
import { ZodTool } from '@engine-ai/javascript/tool-helper';
import { z } from 'zod';

class WeatherTool extends ZodTool {
  constructor() {
    super(
      'get_weather',
      'Get weather information',
      z.object({
        location: z.string().describe('City name')
      }),
      (params) => ({
        location: params.location,
        temperature: 72,
        condition: 'sunny'
      })
    );
  }
}

const engine = ExecutionEngine.withTools([new WeatherTool()]);
```

## API Reference

### ExecutionEngine

#### `new ExecutionEngine()`
Create a new execution engine with no tools.

#### `ExecutionEngine.withTools(tools: Tool[])`
Create a new execution engine with the specified tools.

#### `executeWorkflow(workflowPath: string): Promise<string>`
Execute a workflow from a file path.

#### `executeWorkflowContent(workflowContent: string): Promise<string>`
Execute a workflow from string content.

#### `executeWorkflowWithInputs(workflowPath: string, inputs: string): Promise<string>`
Execute a workflow from a file with input values.

#### `executeWorkflowContentWithInputs(workflowContent: string, inputs: string): Promise<string>`
Execute a workflow from string content with input values.

### Tool Helper (Zod Integration)

#### `createTool<T>(name, description, zodSchema, executeFn)`
Create a tool with Zod schema validation. Returns a fully typed tool.

#### `class ZodTool<T>`
Base class for creating tools with Zod schemas. Extend this class to create reusable tools.

## Examples

The `examples/` directory contains comprehensive examples:

- `01-simple-workflow.ts` - Basic workflow execution
- `02-calculator-tool.ts` - Calculator tool with Zod
- `03-weather-tool-class.ts` - Weather tool using ZodTool class
- `04-workflow-with-inputs.ts` - Workflow with input parameters
- `05-complex-schema.ts` - Complex nested schemas
- `06-array-parameters.ts` - Array parameter handling
- `07-optional-defaults.ts` - Optional and default values
- `08-multiple-tools.ts` - Multiple tools in one workflow

Run examples:
```bash
npm run example:01  # Simple workflow
npm run example:02  # Calculator tool
npm run example:03  # Weather tool class
# ... and so on
```

## Development

Build the native module:
```bash
npm run build
```

Build in debug mode:
```bash
npm run build:debug
```

Run tests:
```bash
npm test
```

## License

MIT
