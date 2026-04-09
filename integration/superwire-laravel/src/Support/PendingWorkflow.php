<?php

namespace Superwire\Laravel\Support;

use Superwire\Laravel\Data\ToolBuildRequest;
use Superwire\Laravel\Data\ToolBuildResult;
use Superwire\Laravel\Data\WorkflowExecutionRequest;
use Superwire\Laravel\Execution\ToolCompiler;
use Superwire\Laravel\Execution\WorkflowExecutor;
use Superwire\Laravel\Exceptions\ToolBuildException;

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
    public function mapInto(string $outputClassName): self
    {
        return $this->outputMapInto($outputClassName);
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
            $toolBuildResult = $this->toolCompiler->build(new ToolBuildRequest($this->toolClasses));
            $this->publishBuiltToolsToWorkflowDirectory($toolBuildResult);
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

    private function publishBuiltToolsToWorkflowDirectory(ToolBuildResult $toolBuildResult): void
    {
        $workflowDirectory = dirname($this->workflowFilePath);
        $workflowToolsDirectory = $workflowDirectory . DIRECTORY_SEPARATOR . 'tools';

        if (!is_dir($workflowToolsDirectory) && !mkdir($workflowToolsDirectory, 0777, true) && !is_dir($workflowToolsDirectory)) {
            throw new ToolBuildException(sprintf('failed to create workflow tools directory %s', $workflowToolsDirectory));
        }

        foreach ($toolBuildResult->toolNames as $toolName) {
            $sourcePath = $toolBuildResult->outputDirectory . DIRECTORY_SEPARATOR . $toolName . '.wasm';
            $destinationPath = $workflowToolsDirectory . DIRECTORY_SEPARATOR . $toolName . '.wasm';

            if (!is_file($sourcePath)) {
                throw new ToolBuildException(sprintf('built tool artifact not found at %s', $sourcePath));
            }

            if (!copy($sourcePath, $destinationPath)) {
                throw new ToolBuildException(sprintf('failed to publish built tool artifact from %s to %s', $sourcePath, $destinationPath));
            }
        }
    }
}
