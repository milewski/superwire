<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Data\Workflow;

use Superwire\Laravel\Data\Workflow\Concerns\ValidatesPayload;

final class AgentData
{
    use ValidatesPayload;

    /**
     * @param list<string> $tools
     * @param list<string> $dependencies
     * @param list<string> $dependents
     * @param mixed $context
     * @param mixed $inference
     * @param mixed $forEach
     */
    public function __construct(
        public readonly string $name,
        public readonly string $provider,
        public readonly mixed $model,
        public readonly mixed $prompt,
        public readonly mixed $context,
        public readonly mixed $inference,
        public readonly array $tools,
        public readonly mixed $forEach,
        public readonly AgentOutputData $output,
        public readonly array $dependencies,
        public readonly array $dependents,
        public readonly int $batch,
    )
    {
    }

    /**
     * @param array<string, mixed> $payload
     */
    public static function fromArray(array $payload): self
    {
        return new self(
            name: self::string($payload, 'name'),
            provider: self::string($payload, 'provider'),
            model: $payload['model'] ?? null,
            prompt: $payload['prompt'] ?? null,
            context: $payload['context'] ?? null,
            inference: $payload['inference'] ?? null,
            tools: self::list($payload, 'tools'),
            forEach: $payload['for_each'] ?? null,
            output: AgentOutputData::fromArray(self::array($payload, 'output')),
            dependencies: self::list($payload, 'dependencies'),
            dependents: self::list($payload, 'dependents'),
            batch: self::int($payload, 'batch'),
        );
    }
}