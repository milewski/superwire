<?php

namespace Superwire\Laravel\Data;

final readonly class WorkflowExecutionRequest
{
    /**
     * @param array<string, mixed> $inputs
     * @param array<string, mixed> $secrets
     * @param list<class-string> $toolClasses
     */
    public function __construct(
        public string $workflowFilePath,
        public array $inputs,
        public array $secrets,
        public array $toolClasses,
    )
    {
    }
}
