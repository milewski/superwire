<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Tests\Feature;

use InvalidArgumentException;
use RuntimeException;
use Superwire\Laravel\Tests\TestCase;
use Superwire\Laravel\Tools\AbstractTool;
use Superwire\Laravel\Tools\AgentToolset;
use Superwire\Laravel\Tools\WorkflowBoundInput;
use Superwire\Laravel\Tools\WorkflowTool;
use Superwire\Laravel\Tools\WorkflowToolInput;

final class ToolAbstractionsTest extends TestCase
{
    public function test_abstract_tool_executes_handle_with_resolved_inputs(): void
    {
        $tool = new ToolAbstractionsTestTool();

        $result = $tool->execute(
            agentInput: new ToolAbstractionsTestInput(topic: 'launch'),
            boundInput: new ToolAbstractionsTestBoundInput(prefix: 'ship'),
        );

        $this->assertSame([ 'message' => 'ship launch' ], $result);
    }

    public function test_abstract_tool_requires_handle_method(): void
    {
        $tool = new class () extends AbstractTool {
        };

        $this->expectException(RuntimeException::class);
        $this->expectExceptionMessage(sprintf('Tool `%s` must define `handle()`.', $tool::class));

        $tool->execute();
    }

    public function test_abstract_tool_requires_array_result(): void
    {
        $tool = new class () extends AbstractTool {
            protected function handle(): string
            {
                return 'not-an-array';
            }
        };

        $this->expectException(RuntimeException::class);
        $this->expectExceptionMessage(sprintf('Tool `%s` must return an array from handle().', $tool::class));

        $tool->execute();
    }

    public function test_agent_toolset_rejects_non_workflow_tools(): void
    {
        $this->expectException(InvalidArgumentException::class);
        $this->expectExceptionMessage(sprintf(
            'Configured tool `%s` must implement %s.',
            ToolAbstractionsInvalidTool::class,
            WorkflowTool::class,
        ));

        AgentToolset::fromArray([ ToolAbstractionsInvalidTool::class ], [ 'type' => 'object' ]);
    }
}

final class ToolAbstractionsTestTool extends AbstractTool
{
    protected function handle(ToolAbstractionsTestInput $agentInput, ToolAbstractionsTestBoundInput $boundInput): array
    {
        return [ 'message' => $boundInput->prefix . ' ' . $agentInput->topic ];
    }
}

final class ToolAbstractionsTestInput extends WorkflowToolInput
{
    public function __construct(
        public string $topic,
    )
    {
    }
}

final class ToolAbstractionsTestBoundInput extends WorkflowBoundInput
{
    public function __construct(
        public string $prefix,
    )
    {
    }
}

final class ToolAbstractionsInvalidTool
{
}
