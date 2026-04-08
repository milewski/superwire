<?php

namespace Superwire\Laravel\Support;

use Superwire\Laravel\Data\ToolBuildRequest;
use Superwire\Laravel\Data\WorkflowExecutionRequest;
use Superwire\Laravel\Execution\ToolCompiler;
use Superwire\Laravel\Execution\WorkflowExecutor;

final readonly class PendingWorkflow
{
    /**
 * @param list<class-string> $toolClasses
 * @param array<string, mixed> $inputs
 * @param array<string, mixed> $secrets
 */
public function __construct(
    private string $workflowFilePath,
    private WorkflowExecutor $workflowExecutor,
    private ToolCompiler $toolCompiler,
    private OutputMapper $outputMapper,
    private array $toolClasses = [],
    private array $inputs = [],
    private array $secrets = [],
    private ?string $outputClassName = null,
)
    {
    }

    /**
     * @param list<class-string> $toolClasses
     */
    public function withTools(array $toolClasses): self
    {
        return new self(
            workflowFilePath: $this->workflowFilePath,
            workflowExecutor: $this->workflowExecutor,
            toolCompiler: $this->toolCompiler,
            outputMapper: $this->outputMapper,
            toolClasses: $toolClasses,
            inputs: $this->inputs,
            secrets: $this->secrets,
            outputClassName: $this->outputClassName,
        );
    }

    /**
     * @param array<string, mixed> $inputs
     */
    public function withInputs(array $inputs): self
    {
        return new self(
            workflowFilePath: $this->workflowFilePath,
            workflowExecutor: $this->workflowExecutor,
            toolCompiler: $this->toolCompiler,
            outputMapper: $this->outputMapper,
            toolClasses: $this->toolClasses,
            inputs: $inputs,
            secrets: $this->secrets,
            outputClassName: $this->outputClassName,
        );
    }

    /**
     * @param array<string, mixed> $secrets
     */
    public function withSecrets(array $secrets): self
    {
        return new self(
            workflowFilePath: $this->workflowFilePath,
            workflowExecutor: $this->workflowExecutor,
            toolCompiler: $this->toolCompiler,
            outputMapper: $this->outputMapper,
            toolClasses: $this->toolClasses,
            inputs: $this->inputs,
            secrets: $secrets,
            outputClassName: $this->outputClassName,
        );
    }

    /**
     * @param class-string $outputClassName
     */
    public function outputMapInto(string $outputClassName): self
    {
        return new self(
            workflowFilePath: $this->workflowFilePath,
            workflowExecutor: $this->workflowExecutor,
            toolCompiler: $this->toolCompiler,
            outputMapper: $this->outputMapper,
            toolClasses: $this->toolClasses,
            inputs: $this->inputs,
            secrets: $this->secrets,
            outputClassName: $outputClassName,
        );
    }

    public function run(): mixed
    {
        if (!empty($this->toolClasses)) {
            $this->toolCompiler->build(new ToolBuildRequest($this->toolClasses));
        }

        $workflowExecutionResult = $this->workflowExecutor->execute(new WorkflowExecutionRequest(
            $this->workflowFilePath,
            $this->inputs,
            $this->secrets,
            $this->toolClasses,
        ));

        if ($this->outputClassName === null) {
            return $workflowExecutionResult->output;
        }

        return $this->outputMapper->mapToClass($workflowExecutionResult->output, $this->outputClassName);
    }
}
