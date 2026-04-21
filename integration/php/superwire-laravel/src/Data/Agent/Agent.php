<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Data\Agent;

use Superwire\Laravel\Data\Concerns\ValidatesPayload;
use Superwire\Laravel\Data\Loop\ForEachData;
use Superwire\Laravel\Data\Prompt\Prompt;
use Superwire\Laravel\Support\JsonSchemaFactory;

final class Agent
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
        public readonly Model $model,
        public readonly Prompt $prompt,
        public readonly Context $context,
        public readonly Inference $inference,
        public readonly array $tools,
        public readonly ?ForEachData $forEach,
        public readonly Output $output,
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
            model: Model::fromValue($payload[ 'model' ] ?? null),
            prompt: Prompt::fromValue($payload[ 'prompt' ] ?? null),
            context: Context::fromValue($payload[ 'context' ] ?? null),
            inference: Inference::fromValue($payload[ 'inference' ] ?? null),
            tools: self::list($payload, 'tools'),
            forEach: ForEachData::fromValue($payload[ 'for_each' ] ?? null),
            output: Output::fromArray(self::array($payload, 'output')),
            dependencies: self::list($payload, 'dependencies'),
            dependents: self::list($payload, 'dependents'),
            batch: self::int($payload, 'batch'),
        );
    }

    public function runsForEach(): bool
    {
        return $this->forEach !== null;
    }

    public function forEachIdentifier(): ?string
    {
        if ($this->forEach === null) {
            return null;
        }

        return $this->forEach->pattern->identifier;
    }

    public function forEachReference(): ?string
    {
        if ($this->forEach === null) {
            return null;
        }

        return $this->forEach->iterable->reference;
    }

    /**
     * @return array<string, mixed>
     */
    public function iterationJsonSchema(): array
    {
        return JsonSchemaFactory::toArray($this->output->iteration->jsonSchema);
    }

    /**
     * @return array<string, mixed>
     */
    public function finalOutputJsonSchema(): array
    {
        return JsonSchemaFactory::toArray($this->output->finalOutput->jsonSchema);
    }
}
