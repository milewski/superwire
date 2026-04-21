<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Data\Workflow;

use InvalidArgumentException;
use Superwire\Laravel\Data\Workflow\Collection\Agents;
use Superwire\Laravel\Data\Workflow\Collection\Providers;
use Superwire\Laravel\Data\Workflow\Collection\Schemas;
use Superwire\Laravel\Data\Workflow\Concerns\ValidatesPayload;

final class WorkflowDefinitionData
{
    use ValidatesPayload;

    public function __construct(
        public readonly string $format,
        public readonly string $workflowPath,
        public readonly ?array $input,
        public readonly ?array $secrets,
        public readonly Schemas $schemas,
        public readonly Providers $providers,
        public readonly Agents $agents,
        public readonly Output $output,
        public readonly Execution $execution,
    )
    {
    }

    /**
     * @param array<string, mixed> $payload
     */
    public static function fromArray(array $payload): self
    {
        return new self(
            format: self::string($payload, 'format'),
            workflowPath: self::string($payload, 'workflow_path'),
            input: $payload[ 'input' ] ?? null,
            secrets: $payload[ 'secrets' ] ?? null,
            schemas: Schemas::fromArray(self::list($payload, 'schemas')),
            providers: Providers::fromArray(self::list($payload, 'providers')),
            agents: Agents::fromArray(self::list($payload, 'agents')),
            output: Output::fromArray(self::array($payload, 'output')),
            execution: Execution::fromArray(self::array($payload, 'execution')),
        );
    }

    public static function fromJson(string $json): self
    {
        $payload = json_decode($json, true);

        if (!is_array($payload)) {
            throw new InvalidArgumentException('json must decode to an array');
        }

        return self::fromArray($payload);
    }
}