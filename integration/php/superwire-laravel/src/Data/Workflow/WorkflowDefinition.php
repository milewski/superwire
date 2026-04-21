<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Data\Workflow;

use InvalidArgumentException;
use Superwire\Laravel\Data\Agent\Execution;
use Superwire\Laravel\Data\Collection\Agents;
use Superwire\Laravel\Data\Collection\Providers;
use Superwire\Laravel\Data\Collection\Schemas;
use Superwire\Laravel\Data\Concerns\ValidatesPayload;

final class WorkflowDefinition
{
    use ValidatesPayload;

    public function __construct(
        public readonly string $format,
        public readonly string $workflowPath,
        public readonly ?WorkflowValueDefinition $input,
        public readonly ?WorkflowValueDefinition $secrets,
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
            input: isset($payload[ 'input' ]) && is_array($payload[ 'input' ]) ? WorkflowValueDefinition::fromArray($payload[ 'input' ]) : null,
            secrets: isset($payload[ 'secrets' ]) && is_array($payload[ 'secrets' ]) ? WorkflowValueDefinition::fromArray($payload[ 'secrets' ]) : null,
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

    /**
     * @param array<string, mixed> $values
     */
    public function validateInputValues(array $values): void
    {
        if ($this->input === null) {

            if ($values !== []) {
                throw new InvalidArgumentException('workflow does not define input');
            }

            return;

        }

        $this->input->validateValues($values, 'input');
    }

    /**
     * @param array<string, mixed> $values
     */
    public function validateSecretValues(array $values): void
    {
        if ($this->secrets === null) {

            if ($values !== []) {
                throw new InvalidArgumentException('workflow does not define secrets');
            }

            return;

        }

        $this->secrets->validateValues($values, 'secrets');
    }
}
