<?php

declare(strict_types = 1);

namespace Superwire\Laravel;

use Prism\Prism\Tool;
use Superwire\Laravel\Concerns\ExecutesWorkflowAgents;
use Superwire\Laravel\Concerns\HandlesForkedWorkflowExecution;
use Superwire\Laravel\Concerns\ResolvesRuntimeProviders;
use Superwire\Laravel\Data\Agent\OutputFieldReference;
use Superwire\Laravel\Data\Workflow\WorkflowDefinition;
use Superwire\Laravel\Support\PromptParser;
use Superwire\Laravel\Tools\WorkflowTool;

final readonly class Runtime
{
    use ExecutesWorkflowAgents;
    use HandlesForkedWorkflowExecution;
    use ResolvesRuntimeProviders;

    public function __construct(
        private WorkflowDefinition $definition,
        private PromptParser $promptParser = new PromptParser(),
        private array $inputValues = [],
        private array $secretValues = [],
        private array $tools = [],
    )
    {
    }

    /**
     * @param array<string, mixed> $inputValues
     */
    public function withInputs(array $inputValues): self
    {
        return new self($this->definition, $this->promptParser, $inputValues, $this->secretValues, $this->tools);
    }

    /**
     * @param array<string, mixed> $secretValues
     */
    public function withSecrets(array $secretValues): self
    {
        return new self($this->definition, $this->promptParser, $this->inputValues, $secretValues, $this->tools);
    }

    /**
     * @param array<int, Tool|WorkflowTool> $tools
     */
    public function withTools(array $tools): self
    {
        return new self($this->definition, $this->promptParser, $this->inputValues, $this->secretValues, $tools);
    }

    public function run(): WorkflowExecutionResult
    {
        $this->definition->validateInputValues($this->inputValues);
        $this->definition->validateSecretValues($this->secretValues);

        $agentOutputs = [];

        foreach ($this->definition->execution->batches as $batchAgentNames) {

            $agentOutputs = array_merge(
                $agentOutputs,
                $this->runBatch($batchAgentNames, $agentOutputs),
            );

        }

        return new WorkflowExecutionResult(
            output: $this->resolveWorkflowOutput($agentOutputs),
            agents: $agentOutputs,
        );
    }

    private function resolveWorkflowOutput(array $agentOutputs): array
    {
        return array_map(
            callback: fn (OutputFieldReference $reference) => $this->resolveOutputField($reference, $agentOutputs),
            array: $this->definition->output->fields,
        );
    }

    private function resolveOutputField(OutputFieldReference $reference, array $agentOutputs): mixed
    {
        return $this->promptParser->resolveReference($reference->ref, $agentOutputs);
    }
}
