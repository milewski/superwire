<?php

declare(strict_types = 1);

namespace Superwire\Laravel;

use RuntimeException;
use Superwire\Contracts\Contracts\DriverRegistryInterface;
use Superwire\Contracts\Contracts\WorkflowRunnerInterface;
use Superwire\Contracts\Support\LoopAgentDriver;
use Superwire\Laravel\Data\WorkflowRunResult;
use Superwire\Laravel\Driver\PrismAgentDriver;
use Superwire\Laravel\Support\CachedWorkflowDefinitionCompiler;
use Superwire\Laravel\Support\LaravelRuntimeToolInvoker;

final class Workflow
{
    /**
     * @param list<class-string> $toolClasses
     * @param array<string, mixed> $driverConfiguration
     * @param array<string, mixed> $input
     * @param array<string, mixed> $secrets
     */
    private function __construct(
        private readonly string $workflowPath,
        private readonly array $toolClasses = [],
        private readonly string $driverName = 'prism',
        private readonly array $driverConfiguration = [],
        private readonly array $input = [],
        private readonly array $secrets = [],
        private readonly ?string $outputClass = null,
    ) {
    }

    public static function fromFile(string $workflowPath): self
    {
        return new self($workflowPath);
    }

    /**
     * @param list<class-string> $toolClasses
     */
    public function withTools(array $toolClasses): self
    {
        return new self(
            workflowPath: $this->workflowPath,
            toolClasses: $toolClasses,
            driverName: $this->driverName,
            driverConfiguration: $this->driverConfiguration,
            input: $this->input,
            secrets: $this->secrets,
            outputClass: $this->outputClass,
        );
    }

    /**
     * @param array<string, mixed> $driverConfiguration
     */
    public function usingDriver(string $driverName, array $driverConfiguration = []): self
    {
        return new self(
            workflowPath: $this->workflowPath,
            toolClasses: $this->toolClasses,
            driverName: $driverName,
            driverConfiguration: $driverConfiguration,
            input: $this->input,
            secrets: $this->secrets,
            outputClass: $this->outputClass,
        );
    }

    /**
     * @param array<string, mixed> $input
     */
    public function withInputs(array $input): self
    {
        return new self(
            workflowPath: $this->workflowPath,
            toolClasses: $this->toolClasses,
            driverName: $this->driverName,
            driverConfiguration: $this->driverConfiguration,
            input: $input,
            secrets: $this->secrets,
            outputClass: $this->outputClass,
        );
    }

    /**
     * @param array<string, mixed> $secrets
     */
    public function withSecrets(array $secrets): self
    {
        return new self(
            workflowPath: $this->workflowPath,
            toolClasses: $this->toolClasses,
            driverName: $this->driverName,
            driverConfiguration: $this->driverConfiguration,
            input: $this->input,
            secrets: $secrets,
            outputClass: $this->outputClass,
        );
    }

    /**
     * @param class-string $outputClass
     */
    public function mapInto(string $outputClass): self
    {
        return new self(
            workflowPath: $this->workflowPath,
            toolClasses: $this->toolClasses,
            driverName: $this->driverName,
            driverConfiguration: $this->driverConfiguration,
            input: $this->input,
            secrets: $this->secrets,
            outputClass: $outputClass,
        );
    }

    public function run(): WorkflowRunResult
    {
        $this->registerExecutionDriver();

        $workflowDefinition = app(CachedWorkflowDefinitionCompiler::class)->compile($this->workflowPath);
        $workflowResult = app(WorkflowRunnerInterface::class)->run(
            $workflowDefinition,
            $this->input,
            $this->resolvedSecrets(),
        );

        return new WorkflowRunResult(
            output: $this->mapOutput($workflowResult->output),
            context: [
                'workflow_output' => $workflowResult->output,
                'agent_outputs' => $workflowResult->agentOutputs,
                'agent_contexts' => $workflowResult->agentContexts,
                'agent_metadata' => $workflowResult->agentMetadata,
                'execution_history' => $workflowResult->executionHistory,
            ],
        );
    }

    private function registerExecutionDriver(): void
    {
        $driverRegistry = app(DriverRegistryInterface::class);

        if ($this->driverName !== 'prism') {
            throw new RuntimeException("unsupported workflow driver `{$this->driverName}`");
        }

        $toolInvoker = app(LaravelRuntimeToolInvoker::class)->withTools($this->toolClasses);
        $driverRegistry->register('prism', new LoopAgentDriver(new PrismAgentDriver($this->driverConfiguration), $toolInvoker));
    }

    /**
     * @return array<string, mixed>
     */
    private function resolvedSecrets(): array
    {
        return $this->secrets;
    }

    private function mapOutput(mixed $workflowOutput): mixed
    {
        if ($this->outputClass === null) {
            return $workflowOutput;
        }

        if (!class_exists($this->outputClass)) {
            throw new RuntimeException("mapped output class `{$this->outputClass}` does not exist");
        }

        if (!is_array($workflowOutput)) {
            throw new RuntimeException('workflow output must be an array to map into output class');
        }

        $outputClass = $this->outputClass;

        if (method_exists($outputClass, 'from')) {
            return $outputClass::from($workflowOutput);
        }

        return new $outputClass(...$workflowOutput);
    }
}
