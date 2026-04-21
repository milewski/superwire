<?php

declare(strict_types = 1);

namespace Superwire\Laravel;

use Prism\Prism\Tool;
use Superwire\Laravel\Data\Workflow\WorkflowDefinition;
use Superwire\Laravel\Tools\WorkflowTool;

final readonly class Workflow
{
    /**
     * @param array<string, mixed> $inputValues
     * @param array<string, mixed> $secretValues
     * @param array<int, Tool|WorkflowTool> $tools
     */
    private function __construct(
        private string $workflowPath,
        private array $inputValues = [],
        private array $secretValues = [],
        private array $tools = [],
    ) {
    }

    public static function fromFile(string $workflowPath): self
    {
        return new self($workflowPath);
    }

    /**
     * @param array<string, mixed> $inputValues
     */
    public function withInputs(array $inputValues): self
    {
        return new self($this->workflowPath, $inputValues, $this->secretValues, $this->tools);
    }

    /**
     * @param array<string, mixed> $secretValues
     */
    public function withSecrets(array $secretValues): self
    {
        return new self($this->workflowPath, $this->inputValues, $secretValues, $this->tools);
    }

    /**
     * @param array<int, Tool|WorkflowTool> $tools
     */
    public function withTools(array $tools): self
    {
        return new self($this->workflowPath, $this->inputValues, $this->secretValues, $tools);
    }

    public function definition(): WorkflowDefinition
    {
        return app(WorkflowCompiler::class)->compile($this->workflowPath);
    }

    public function runtime(): Runtime
    {
        return (new Runtime($this->definition()))
            ->withInputs($this->inputValues)
            ->withSecrets($this->secretValues)
            ->withTools($this->tools);
    }

    public function run(): WorkflowExecutionResult
    {
        return $this->runtime()->run();
    }
}
