<?php

declare(strict_types = 1);

namespace Superwire\Contracts\Tests\Fakes;

use RuntimeException;
use Superwire\Contracts\Agent\AgentExecutionRequest;
use Superwire\Contracts\Agent\AgentToolCall;
use Superwire\Contracts\Agent\AgentToolResult;
use Superwire\Contracts\Contracts\RuntimeToolInvokerInterface;
use Superwire\Contracts\Contracts\RuntimeToolMetadataProviderInterface;
use Superwire\Contracts\Contracts\RuntimeToolSchemaProviderInterface;
use Swaggest\JsonSchema\Schema;

final class RecordingRuntimeToolInvoker implements RuntimeToolInvokerInterface, RuntimeToolMetadataProviderInterface, RuntimeToolSchemaProviderInterface
{
    /**
     * @param class-string|null $inputSchema
     * @param class-string|null $boundedSchema
     * @param array<string, mixed> $output
     */
    public static function fake(
        string $name = 'fake_tool',
        ?string $inputSchema = null,
        ?string $boundedSchema = null,
        array $output = [],
        string $id = 'tool-call-1',
        ?string $description = null,
        ?bool $strict = null,
    ): self
    {
        return new self($name, $inputSchema, $boundedSchema, $output, $id, $description, $strict);
    }

    /**
     * @var list<array{request: AgentExecutionRequest, tool_call: AgentToolCall}>
     */
    public array $invocations = [];

    /**
     * @param class-string|null $inputSchemaClass
     * @param class-string|null $boundedSchemaClass
     * @param array<string, mixed> $output
     */
    public function __construct(
        private readonly string $name,
        private readonly ?string $inputSchemaClass,
        private readonly ?string $boundedSchemaClass,
        private readonly array $output = [],
        private readonly string $id = 'tool-call-1',
        private readonly ?string $description = null,
        private readonly ?bool $strict = null,
    ) {
    }

    public function id(): string
    {
        return $this->id;
    }

    public function name(): string
    {
        return $this->name;
    }

    public function schemaForTool(string $toolName): ?Schema
    {
        if ($toolName !== $this->name) {
            return null;
        }

        if ($this->inputSchemaClass === null) {
            return null;
        }

        if (!method_exists($this->inputSchemaClass, 'schema')) {
            throw new RuntimeException("invalid input schema class `{$this->inputSchemaClass}`: missing schema()");
        }

        $schema = $this->inputSchemaClass::schema();

        if (!$schema instanceof Schema) {
            throw new RuntimeException("invalid input schema class `{$this->inputSchemaClass}`: schema() must return Schema");
        }

        return $schema;
    }

    public function descriptionForTool(string $toolName): ?string
    {
        if ($toolName !== $this->name) {
            return null;
        }

        return $this->description;
    }

    public function strictSchemaForTool(string $toolName): ?bool
    {
        if ($toolName !== $this->name) {
            return null;
        }

        return $this->strict;
    }

    public function invoke(AgentExecutionRequest $request, AgentToolCall $toolCall): AgentToolResult
    {
        if ($toolCall->name !== $this->name) {
            throw new RuntimeException("unexpected tool call `{$toolCall->name}`, expected `{$this->name}`");
        }

        $this->invocations[] = [
            'request' => $request,
            'tool_call' => $toolCall,
        ];

        if ($this->inputSchemaClass !== null) {
            $this->validateSchemaClass($this->inputSchemaClass, $toolCall->arguments, 'input');
        }

        if ($this->boundedSchemaClass !== null) {

            $this->validateSchemaClass(
                $this->boundedSchemaClass,
                $this->resolvedBoundedArguments($request, $toolCall->name),
                'bounded',
            );

        }

        return new AgentToolResult(
            toolCallId: $toolCall->id,
            toolName: $toolCall->name,
            arguments: $toolCall->arguments,
            result: $this->output !== [] ? $this->output : [
                'status' => 'ok',
            ],
        );
    }

    /**
     * @param array<string, mixed> $arguments
     */
    private function validateSchemaClass(string $schemaClass, array $arguments, string $schemaType): void
    {
        if (!method_exists($schemaClass, 'validate')) {
            throw new RuntimeException("invalid {$schemaType} schema class `{$schemaClass}`: missing validate()");
        }

        $schemaClass::validate($arguments);
    }

    /**
     * @return array<string, mixed>
     */
    private function resolvedBoundedArguments(AgentExecutionRequest $request, string $toolName): array
    {
        foreach ($request->tools as $toolExecution) {

            if ($toolExecution->name === $toolName) {
                return $toolExecution->bindings;
            }

        }

        return [];
    }
}
