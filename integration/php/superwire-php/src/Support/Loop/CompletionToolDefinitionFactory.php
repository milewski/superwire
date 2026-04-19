<?php

declare(strict_types = 1);

namespace Superwire\Contracts\Support\Loop;

use Superwire\Contracts\Agent\AgentExecutionRequest;
use Superwire\Contracts\Agent\AgentToolDefinition;
use Superwire\Contracts\Support\Stages\CompletionToolLoopStage;

final readonly class CompletionToolDefinitionFactory
{
    public function __construct(
        private CompletionToolLoopStage $completionToolLoopStage,
    )
    {
    }

    public function finalizeSuccessTool(AgentExecutionRequest $request): AgentToolDefinition
    {
        return new AgentToolDefinition(
            name: $this->completionToolLoopStage->finalizeSuccessToolName(),
            description: 'Call this only when the task is completed successfully and provide final answer',
            parametersSchema: [
                'type' => 'object',
                'properties' => [
                    'answer' => $request->expectedOutput->jsonSchema,
                ],
                'required' => [ 'answer' ],
                'additionalProperties' => false,
            ],
        );
    }

    public function finalizeErrorTool(): AgentToolDefinition
    {
        return new AgentToolDefinition(
            name: $this->completionToolLoopStage->finalizeErrorToolName(),
            description: 'Call this only when task cannot be completed and provide reason',
            parametersSchema: [
                'type' => 'object',
                'properties' => [
                    'reason' => [ 'type' => 'string' ],
                ],
                'required' => [ 'reason' ],
                'additionalProperties' => false,
            ],
        );
    }
}
