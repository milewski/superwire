<?php

declare(strict_types = 1);

namespace Superwire\Contracts\Support\Stages;

use Superwire\Contracts\Agent\AgentToolCall;

final readonly class CompletionToolDecision
{
    /**
     * @param list<AgentToolCall> $runtimeToolCalls
     */
    private function __construct(
        public CompletionToolDecisionStatus $status,
        public array $runtimeToolCalls,
        public mixed $output = null,
        public ?string $reason = null,
    )
    {
    }

    /**
     * @param list<AgentToolCall> $runtimeToolCalls
     */
    public static function continue(array $runtimeToolCalls): self
    {
        return new self(CompletionToolDecisionStatus::Continue, $runtimeToolCalls);
    }

    public static function success(mixed $output): self
    {
        return new self(CompletionToolDecisionStatus::Success, [], output: $output);
    }

    public static function error(string $reason): self
    {
        return new self(CompletionToolDecisionStatus::Error, [], reason: $reason);
    }

    public function isSuccess(): bool
    {
        return $this->status === CompletionToolDecisionStatus::Success;
    }

    public function isError(): bool
    {
        return $this->status === CompletionToolDecisionStatus::Error;
    }
}
