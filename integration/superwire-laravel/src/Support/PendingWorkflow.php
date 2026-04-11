<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Support;

use Superwire\Laravel\Contracts\Tool;
use Superwire\Laravel\Data\WorkflowExecutionRequest;
use Superwire\Laravel\Exceptions\WorkflowExecutionException;
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
            $this->assertCompiledToolsAreAvailable();
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

    private function assertCompiledToolsAreAvailable(): void
    {
        $workflowDirectory = dirname($this->workflowFilePath);
        $workflowToolsDirectory = $workflowDirectory . DIRECTORY_SEPARATOR . 'tools';

        if (!is_dir($workflowToolsDirectory)) {

            throw new WorkflowExecutionException(sprintf(
                'missing compiled workflow tools directory `%s`; run `php artisan superwire:tools:prepare --workflow=%s` before execution',
                $workflowToolsDirectory,
                $this->workflowFilePath,
            ));

        }

        $missingToolArtifacts = [];

        foreach ($this->validatedToolClasses() as $toolClass) {

            $toolName = $toolClass::name();
            $toolArtifactPath = $workflowToolsDirectory . DIRECTORY_SEPARATOR . $toolName . '.wasm';

            if (!is_file($toolArtifactPath)) {
                $missingToolArtifacts[] = $toolName;
            }

        }

        if ($missingToolArtifacts !== []) {

            throw new WorkflowExecutionException(sprintf(
                'missing compiled tool artifact(s) for workflow `%s`: %s. Run `php artisan superwire:tools:prepare --workflow=%s` before execution',
                $this->workflowFilePath,
                implode(', ', array_map(static fn (string $toolName): string => sprintf('tool.%s', $toolName), $missingToolArtifacts)),
                $this->workflowFilePath,
            ));

        }
    }

    /**
     * @return list<class-string<Tool>>
     */
    private function validatedToolClasses(): array
    {
        $validatedToolClasses = [];

        foreach ($this->toolClasses as $toolClass) {

            if (!is_string($toolClass)) {
                throw new WorkflowExecutionException('tool class references must be class-string values');
            }

            if (!class_exists($toolClass)) {
                throw new WorkflowExecutionException(sprintf('tool class `%s` does not exist', $toolClass));
            }

            if (!is_subclass_of($toolClass, Tool::class)) {
                throw new WorkflowExecutionException(sprintf('tool class `%s` must implement %s', $toolClass, Tool::class));
            }

            $validatedToolClasses[] = $toolClass;

        }

        return $validatedToolClasses;
    }
}
