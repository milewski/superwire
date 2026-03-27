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
        input {
          name: string
        }

        output {
          greeting: input.name
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
