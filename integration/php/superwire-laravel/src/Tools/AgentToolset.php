<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Tools;

use Prism\Prism\Tool;
use RuntimeException;
use Superwire\Laravel\AgentExecutionResult;

final class AgentToolset
{
    /**
     * @param array<int, WorkflowTool> $userTools
     */
    private function __construct(
        private array $userTools,
        private FinalizeSuccessTool $finalizeSuccessTool,
        private FinalizeErrorTool $finalizeErrorTool,
    )
    {
    }

    /**
     * @param array<int, Tool|WorkflowTool> $tools
     * @param array<string, mixed> $outputSchema
     */
    public static function fromArray(array $tools, array $outputSchema): self
    {
        return new self(
            userTools: array_map(self::normalizeTool(...), $tools),
            finalizeSuccessTool: new FinalizeSuccessTool($outputSchema),
            finalizeErrorTool: new FinalizeErrorTool(),
        );
    }

    /**
     * @return array<int, Tool>
     */
    public function prismTools(array $boundArguments = []): array
    {
        return array_map(
            static fn (WorkflowTool $tool): Tool => $tool->toPrismTool($boundArguments),
            [ ...$this->userTools, $this->finalizeSuccessTool, $this->finalizeErrorTool ],
        );
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

    private static function normalizeTool(Tool|WorkflowTool $tool): WorkflowTool
    {
        if ($tool instanceof WorkflowTool) {
            return $tool;
        }

        return new PrismWorkflowTool($tool);
    }
}
