<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Tools;

use InvalidArgumentException;
use Prism\Prism\Tool;
use RuntimeException;
use Superwire\Laravel\AgentExecutionResult;
use Superwire\Laravel\Tools\Internal\FinalizeErrorTool;
use Superwire\Laravel\Tools\Internal\FinalizeSuccessTool;

final readonly class AgentToolset
{
    /**
     * @param array<int, array{tool: WorkflowTool, bound_arguments: array<string, mixed>}> $userTools
     */
    private function __construct(
        private array $userTools,
        private FinalizeSuccessTool $finalizeSuccessTool,
        private FinalizeErrorTool $finalizeErrorTool,
    )
    {
    }

    /**
     * @param array<int, string|WorkflowTool> $tools
     * @param array<string, mixed> $outputSchema
     * @param array<string, array<string, mixed>> $toolBindings
     */
    public static function fromArray(array $tools, array $outputSchema, array $toolBindings = []): self
    {
        $normalizedTools = [];

        foreach ($tools as $tool) {

            $workflowTool = self::normalizeTool($tool);
            $normalizedTools[] = [
                'tool' => $workflowTool,
                'bound_arguments' => $toolBindings[ $workflowTool::name() ] ?? [],
            ];

        }

        return new self(
            userTools: $normalizedTools,
            finalizeSuccessTool: new FinalizeSuccessTool($outputSchema),
            finalizeErrorTool: new FinalizeErrorTool(),
        );
    }

    /**
     * @return array<int, Tool>
     */
    public function prismTools(): array
    {
        $tools = [];

        foreach ($this->userTools as $boundTool) {
            $tools[] = $boundTool[ 'tool' ]->toPrismTool($boundTool[ 'bound_arguments' ]);
        }

        $tools[] = $this->finalizeSuccessTool->toPrismTool();
        $tools[] = $this->finalizeErrorTool->toPrismTool();

        return $tools;
    }

    public function resetFinalization(): void
    {
        $this->finalizeSuccessTool->reset();
        $this->finalizeErrorTool->reset();
    }

    /**
     * @param array<int, array<string, mixed>> $messages
     */
    public function finalizeExecutionResult(string $agentName, array $messages): ?AgentExecutionResult
    {
        if ($this->finalizeErrorTool->wasCalled()) {

            throw new RuntimeException(
                message: sprintf('Agent %s failed: %s', $agentName, $this->finalizeErrorTool->reason()),
            );

        }

        if (!$this->finalizeSuccessTool->wasCalled()) {
            return null;
        }

        return new AgentExecutionResult(
            output: $this->finalizeSuccessTool->result(),
            messages: $messages,
        );
    }

    private static function normalizeTool(WorkflowTool|string $tool): WorkflowTool
    {
        if (is_string($tool)) {
            $tool = app($tool);
        }

        if ($tool instanceof WorkflowTool) {
            return $tool;
        }

        throw new InvalidArgumentException(sprintf(
            'Configured tool `%s` must implement %s.',
            is_object($tool) ? $tool::class : gettype($tool),
            WorkflowTool::class,
        ));
    }
}
