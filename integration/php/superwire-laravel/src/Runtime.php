<?php

declare(strict_types = 1);

namespace Superwire\Laravel;

use Spatie\LaravelData\Data;
use Superwire\Laravel\Concerns\ExecutesWorkflowAgents;
use Superwire\Laravel\Concerns\HandlesForkedWorkflowExecution;
use Superwire\Laravel\Concerns\ResolvesRuntimeProviders;
use Superwire\Laravel\Concerns\ResolvesWorkflowTools;
use Superwire\Laravel\Data\Agent\OutputFieldReference;
use Superwire\Laravel\Data\Workflow\WorkflowDefinition;
use Superwire\Laravel\Support\PromptParser;
use Superwire\Laravel\Tools\WorkflowTool;

final readonly class Runtime
{
    use ExecutesWorkflowAgents;
    use HandlesForkedWorkflowExecution;
    use ResolvesRuntimeProviders;
    use ResolvesWorkflowTools;

    public function __construct(
        private WorkflowDefinition $definition,
        private PromptParser $promptParser = new PromptParser(),
        private array $inputValues = [],
        private array $secretValues = [],
        private array $tools = [],
        private ?string $outputClass = null,
    )
    {
    }

    /**
     * @param array<string, mixed> $inputValues
     */
    public function withInputs(array $inputValues): self
    {
        return new self(
            definition: $this->definition,
            promptParser: $this->promptParser,
            inputValues: $inputValues,
            secretValues: $this->secretValues,
            tools: $this->tools,
            outputClass: $this->outputClass,
        );
    }

    /**
     * @param array<string, mixed> $secretValues
     */
    public function withSecrets(array $secretValues): self
    {
        return new self(
            definition: $this->definition,
            promptParser: $this->promptParser,
            inputValues: $this->inputValues,
            secretValues: $secretValues,
            tools: $this->tools,
            outputClass: $this->outputClass,
        );
    }

    /**
     * @param array<int, string|WorkflowTool> $tools
     */
    public function withTools(array $tools): self
    {
        return new self(
            definition: $this->definition,
            promptParser: $this->promptParser,
            inputValues: $this->inputValues,
            secretValues: $this->secretValues,
            tools: $tools,
            outputClass: $this->outputClass,
        );
    }

    /**
     * @param class-string|null $outputClass
     */
    public function mapInto(?string $outputClass): self
    {
        return new self(
            definition: $this->definition,
            promptParser: $this->promptParser,
            inputValues: $this->inputValues,
            secretValues: $this->secretValues,
            tools: $this->tools,
            outputClass: $outputClass,
        );
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

        $workflowOutput = $this->resolveWorkflowOutput($agentOutputs);

        return new WorkflowExecutionResult(
            output: $this->mapOutput($workflowOutput),
            agents: $agentOutputs,
            context: [
                'agent_outputs' => array_map(
                    static fn (AgentExecutionResult $agentExecutionResult): mixed => $agentExecutionResult->output,
                    $agentOutputs,
                ),
                'inputs' => $this->inputValues,
                'secrets' => $this->secretValues,
            ],
        );
    }

    /**
     * @param array<string, mixed> $workflowOutput
     */
    private function mapOutput(array $workflowOutput): mixed
    {
        if ($this->outputClass === null) {
            return $workflowOutput;
        }

        if (is_subclass_of($this->outputClass, Data::class)) {
            return $this->outputClass::from($workflowOutput);
        }

        if (method_exists($this->outputClass, 'from')) {
            return $this->outputClass::from($workflowOutput);
        }

        return new ($this->outputClass)(...$workflowOutput);
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
