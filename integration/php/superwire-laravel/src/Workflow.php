<?php

declare(strict_types = 1);

namespace Superwire\Laravel;

use Superwire\Laravel\Data\Workflow\WorkflowDefinition;
use Superwire\Laravel\Tools\WorkflowTool;

final readonly class Workflow
{
    /**
     * @param array<string, mixed> $inputValues
     * @param array<string, mixed> $secretValues
     * @param array<int, string|WorkflowTool> $tools
     */
    private function __construct(
        private ?string $workflowPath = null,
        private ?WorkflowDefinition $workflowDefinition = null,
        private array $inputValues = [],
        private array $secretValues = [],
        private array $tools = [],
        private ?string $outputClass = null,
    )
    {
    }

    public static function fromFile(string $workflowPath): self
    {
        return new self(workflowPath: $workflowPath);
    }

    public static function fromSource(string $workflowSource, ?string $workflowPath = null): self
    {
        return new self(
            workflowPath: $workflowPath,
            workflowDefinition: app(WorkflowCompiler::class)->compileSource($workflowSource, $workflowPath),
        );
    }

    /**
     * @param array<string, mixed> $inputValues
     */
    public function withInputs(array $inputValues): self
    {
        return new self($this->workflowPath, $this->workflowDefinition, $inputValues, $this->secretValues, $this->tools, $this->outputClass);
    }

    /**
     * @param array<string, mixed> $secretValues
     */
    public function withSecrets(array $secretValues): self
    {
        return new self($this->workflowPath, $this->workflowDefinition, $this->inputValues, $secretValues, $this->tools, $this->outputClass);
    }

    /**
     * @param array<int, string|WorkflowTool> $tools
     */
    public function withTools(array $tools): self
    {
        return new self($this->workflowPath, $this->workflowDefinition, $this->inputValues, $this->secretValues, $tools, $this->outputClass);
    }

    /**
     * @param class-string $outputClass
     */
    public function mapInto(string $outputClass): self
    {
        return new self($this->workflowPath, $this->workflowDefinition, $this->inputValues, $this->secretValues, $this->tools, $outputClass);
    }

    public function definition(): WorkflowDefinition
    {
        return $this->workflowDefinition ?? app(WorkflowCompiler::class)->compile((string) $this->workflowPath);
    }

    public function runtime(): Runtime
    {
        return (new Runtime($this->definition()))
            ->withInputs($this->inputValues)
            ->withSecrets($this->secretValues)
            ->withTools($this->tools)
            ->mapInto($this->outputClass);
    }

    public function run(): WorkflowExecutionResult
    {
        return $this->runtime()->run();
    }
}
