<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Tests;

use RuntimeException;
use Superwire\Contracts\Agent\AgentExecutionRequest;
use Superwire\Contracts\Agent\AgentExpectedOutput;
use Superwire\Contracts\Agent\AgentToolCall;
use Superwire\Contracts\Provider\ProviderExecution;
use Superwire\Contracts\Tool\ToolExecution;
use Superwire\Laravel\Support\LaravelRuntimeToolInvoker;
use Superwire\Laravel\Tools\Attributes\Description;
use Superwire\Laravel\Tools\WorkflowTool;
use Superwire\Laravel\Tools\WorkflowToolBoundInput;
use Superwire\Laravel\Tools\WorkflowToolInput;
use Superwire\Laravel\Tools\WorkflowToolResult;
use Swaggest\JsonSchema\Schema;

final class LaravelRuntimeToolInvokerTest extends TestCase
{
    public function testItResolvesTypedBoundAndInputObjects(): void
    {
        $runtimeToolInvoker = (new LaravelRuntimeToolInvoker($this->app))->withTools([ TypedWorkflowTool::class ]);
        $agentExecutionRequest = $this->requestForTool('typed_workflow_tool', [ 'workspace_id' => 10 ]);
        $toolCall = new AgentToolCall(
            id: 'tool-call-1',
            name: 'typed_workflow_tool',
            arguments: [
                'participant_id' => 99,
                'include_archived' => true,
            ],
        );

        $agentToolResult = $runtimeToolInvoker->invoke($agentExecutionRequest, $toolCall);

        $this->assertSame('typed_workflow_tool', $agentToolResult->toolName);
        $this->assertInstanceOf(WorkflowToolResult::class, $agentToolResult->result);
        $this->assertSame(
            [
                'status' => 'success',
                'payload' => [
                    'workspace_id' => 10,
                    'participant_id' => 99,
                    'include_archived' => true,
                ],
            ],
            json_decode((string) json_encode($agentToolResult->result), true),
        );
    }

    public function testItResolvesOptionalTypedInputAsNullWhenArgumentsAreMissing(): void
    {
        $runtimeToolInvoker = (new LaravelRuntimeToolInvoker($this->app))->withTools([ OptionalInputWorkflowTool::class ]);
        $agentExecutionRequest = $this->requestForTool('optional_input_workflow_tool', [ 'workspace_id' => 22 ]);
        $toolCall = new AgentToolCall(
            id: 'tool-call-2',
            name: 'optional_input_workflow_tool',
            arguments: [],
        );

        $agentToolResult = $runtimeToolInvoker->invoke($agentExecutionRequest, $toolCall);

        $this->assertSame(
            [
                'workspace_id' => 22,
                'has_input' => false,
            ],
            $agentToolResult->result,
        );
    }

    public function testItExposesJsonSchemaForTypedInputClass(): void
    {
        $runtimeToolInvoker = (new LaravelRuntimeToolInvoker($this->app))->withTools([ TypedWorkflowTool::class ]);
        $schema = $runtimeToolInvoker->schemaForTool('typed_workflow_tool');
        $schemaArray = $schema !== null ? $this->schemaToArray($schema) : null;

        $this->assertNotNull($schemaArray);
        $this->assertSame('object', $schemaArray[ 'type' ] ?? null);
        $this->assertSame('integer', $schemaArray[ 'properties' ][ 'participant_id' ][ 'type' ] ?? null);
        $this->assertSame('Participant identifier for answer lookup.', $schemaArray[ 'properties' ][ 'participant_id' ][ 'description' ] ?? null);
        $this->assertSame('boolean', $schemaArray[ 'properties' ][ 'include_archived' ][ 'type' ] ?? null);
        $this->assertSame([ 'participant_id' ], $schemaArray[ 'required' ] ?? []);
    }

    public function testItReturnsStandardizedFailurePayload(): void
    {
        $runtimeToolInvoker = (new LaravelRuntimeToolInvoker($this->app))->withTools([ FailingWorkflowTool::class ]);
        $agentExecutionRequest = $this->requestForTool('failing_workflow_tool', [ 'workspace_id' => 42 ]);
        $toolCall = new AgentToolCall(
            id: 'tool-call-3',
            name: 'failing_workflow_tool',
            arguments: [
                'participant_id' => 13,
            ],
        );

        $agentToolResult = $runtimeToolInvoker->invoke($agentExecutionRequest, $toolCall);

        $this->assertSame(
            [
                'status' => 'error',
                'error' => [
                    'reason' => 'task answer not found',
                    'details' => [
                        'participant_id' => 13,
                        'workspace_id' => 42,
                    ],
                ],
            ],
            json_decode((string) json_encode($agentToolResult->result), true),
        );
    }

    public function testItUsesToolProvidedDescriptionAndStrictMetadata(): void
    {
        $runtimeToolInvoker = (new LaravelRuntimeToolInvoker($this->app))->withTools([ TypedWorkflowTool::class ]);

        $this->assertSame('Retrieve one participant answer for a task', $runtimeToolInvoker->descriptionForTool('typed_workflow_tool'));
        $this->assertTrue($runtimeToolInvoker->strictSchemaForTool('typed_workflow_tool'));
    }

    public function testItWrapsUnexpectedToolExceptionsAsFailurePayload(): void
    {
        $runtimeToolInvoker = (new LaravelRuntimeToolInvoker($this->app))->withTools([ ThrowingWorkflowTool::class ]);
        $agentExecutionRequest = $this->requestForTool('throwing_workflow_tool', [ 'workspace_id' => 7 ]);
        $toolCall = new AgentToolCall(
            id: 'tool-call-4',
            name: 'throwing_workflow_tool',
            arguments: [
                'participant_id' => 55,
            ],
        );

        $agentToolResult = $runtimeToolInvoker->invoke($agentExecutionRequest, $toolCall);

        $this->assertSame(
            [
                'status' => 'error',
                'error' => [
                    'reason' => 'runtime tool invocation failed',
                    'details' => [
                        'tool' => 'throwing_workflow_tool',
                        'reason' => 'unexpected exception while building tool output',
                    ],
                ],
            ],
            json_decode((string) json_encode($agentToolResult->result), true),
        );
    }

    private function requestForTool(string $toolName, array $bindings): AgentExecutionRequest
    {
        return new AgentExecutionRequest(
            agentName: 'agent',
            provider: new ProviderExecution(
                name: 'openai',
                provider: 'openai',
                config: [],
            ),
            model: 'gpt-4.1-mini',
            prompt: 'run tool',
            expectedOutput: new AgentExpectedOutput(
                workflowType: [ 'kind' => 'string' ],
                jsonSchema: Schema::string(),
            ),
            tools: [ new ToolExecution($toolName, $bindings) ],
        );
    }

    /**
     * @return array<string, mixed>
     */
    private function schemaToArray(Schema $schema): array
    {
        $decodedSchema = json_decode(json_encode($schema, JSON_THROW_ON_ERROR), true, 512, JSON_THROW_ON_ERROR);

        $this->assertIsArray($decodedSchema);

        return $decodedSchema;
    }
}

final class TypedWorkflowToolBoundInput extends WorkflowToolBoundInput
{
    public function __construct(
        public int $workspace_id,
    ) {
    }
}

final class TypedWorkflowToolInput extends WorkflowToolInput
{
    public function __construct(
        #[Description('Participant identifier for answer lookup.')]
        public int $participant_id,
        public bool $include_archived = false,
    ) {
    }
}

final class OptionalWorkflowToolInput extends WorkflowToolInput
{
    public function __construct(
        public string $task_id,
    ) {
    }
}

final class TypedWorkflowTool extends WorkflowTool
{
    public static function toolDescription(): string
    {
        return 'Retrieve one participant answer for a task';
    }

    public function invoke(TypedWorkflowToolBoundInput $boundInput, TypedWorkflowToolInput $input): WorkflowToolResult
    {
        return $this->success([
            'workspace_id' => $boundInput->workspace_id,
            'participant_id' => $input->participant_id,
            'include_archived' => $input->include_archived,
        ]);
    }
}

final class OptionalInputWorkflowTool extends WorkflowTool
{
    public function invoke(TypedWorkflowToolBoundInput $boundInput, ?OptionalWorkflowToolInput $input = null): array
    {
        return [
            'workspace_id' => $boundInput->workspace_id,
            'has_input' => $input !== null,
        ];
    }
}

final class FailingWorkflowTool extends WorkflowTool
{
    public function invoke(TypedWorkflowToolBoundInput $boundInput, TypedWorkflowToolInput $input): WorkflowToolResult
    {
        return $this->fail('task answer not found', [
            'participant_id' => $input->participant_id,
            'workspace_id' => $boundInput->workspace_id,
        ]);
    }
}

final class ThrowingWorkflowTool extends WorkflowTool
{
    public function invoke(TypedWorkflowToolBoundInput $boundInput, TypedWorkflowToolInput $input): WorkflowToolResult
    {
        throw new RuntimeException('unexpected exception while building tool output');
    }
}
