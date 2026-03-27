import { createEngineFfiBridge } from "../index";

type WorkflowExecutionEnvelope =
  | {
      status: "succeeded";
      output: {
        execution_id: string;
        output: Record<string, unknown>;
      };
    }
  | {
      status: "failed";
      error: {
        code: string;
        message: string;
        details?: unknown;
      };
    };

function runSimpleExample(): void {
  const engineFfiBridge = createEngineFfiBridge();

  const workflowExecutionEnvelope = engineFfiBridge.executeWorkflow(
    {
      execution_id: "typescript-example-execution",
      workflow_source: `
        provider openai_local {
          driver: "openai"
          endpoint: "http://169.254.83.107:1234/v1"
          api_key: "local-api-key"
          models: ["qwen3.5-9b"]
        }

        input {
          name: string
        }

        agent greeter {
          model: openai_local("qwen3.5-9b")
          prompt: "Tell me a joke"
          output: string
        }

        output {
          greeting: agent.greeter
        }
      `,
      input: {
        payload: {
          name: "TypeScript",
        },
      },
      custom_tools: [],
    },
    {
      requestId: "typescript-example-request",
    },
  ) as WorkflowExecutionEnvelope;

  if (workflowExecutionEnvelope.status === "failed") {
    throw new Error(
      `Workflow failed: ${workflowExecutionEnvelope.error.code} ${workflowExecutionEnvelope.error.message}`,
    );
  }

  process.stdout.write(`${JSON.stringify(workflowExecutionEnvelope.output.output, null, 2)}\n`);

  engineFfiBridge.close();
}

runSimpleExample();
