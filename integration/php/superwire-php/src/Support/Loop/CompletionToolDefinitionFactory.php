<?php

declare(strict_types = 1);

namespace Superwire\Contracts\Support\Loop;

use Superwire\Contracts\Agent\AgentExecutionRequest;
use Superwire\Contracts\Agent\AgentToolDefinition;
use Superwire\Contracts\Support\Stages\CompletionToolLoopStage;
use Swaggest\JsonSchema\Schema;

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
            parametersSchema: Schema::object()
                ->setProperty('answer', $request->expectedOutput->jsonSchema)
                ->setRequired([ 'answer' ])
                ->setAdditionalProperties(false),
        );
    }

    public function finalizeErrorTool(): AgentToolDefinition
    {
        return new AgentToolDefinition(
            name: $this->completionToolLoopStage->finalizeErrorToolName(),
            description: 'Call this only when task cannot be completed and provide reason',
            parametersSchema: Schema::object()
                ->setProperty('reason', Schema::string())
                ->setRequired([ 'reason' ])
                ->setAdditionalProperties(false),
        );
    }
}
