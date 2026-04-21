<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Tools;

use Illuminate\Support\Str;
use Prism\Prism\Schema\RawSchema;
use Prism\Prism\Tool;
use RuntimeException;
use Spatie\LaravelData\Contracts\BaseData;
use Superwire\Laravel\Contracts\ToolBoundInputData;
use Superwire\Laravel\Contracts\ToolInputData;
use Superwire\Laravel\Support\JsonSchemaFactory;
use Superwire\Laravel\Tools\Concerns\InfersToolInputSchemas;
use Superwire\Laravel\Tools\Concerns\ReflectsToolSignature;
use Superwire\Laravel\Tools\Concerns\ResolvesToolDescriptions;

abstract class AbstractTool implements WorkflowTool
{
    use InfersToolInputSchemas;
    use ReflectsToolSignature;
    use ResolvesToolDescriptions;

    public function name(): string
    {
        return Str::snake(class_basename(static::class));
    }

    public static function description(): string
    {
        $description = static::descriptionFromClassAttributes();

        if ($description !== null) {
            return $description;
        }

        return sprintf('Use `%s` to complete this action.', Str::headline(class_basename(static::class)));
    }

    public function toPrismTool(array $boundArguments = []): Tool
    {
        $tool = new Tool();

        $tool
            ->as($this->name())
            ->for(static::description())
            ->withoutErrorHandling();

        foreach ($this->agentInputSchemas() as $parameterSchema) {

            $tool->withParameter(
                parameter: new RawSchema($parameterSchema[ 'name' ], JsonSchemaFactory::toArray($parameterSchema[ 'schema' ])),
                required: $parameterSchema[ 'required' ],
            );

        }

        return $tool->using(function (...$agentArguments) use ($boundArguments): string {

            $result = $this->execute(
                agentInput: static::resolveAgentInput($agentArguments),
                boundInput: static::resolveBoundInput($boundArguments),
            );

            return json_encode($result, JSON_THROW_ON_ERROR);

        });
    }

    public function execute(mixed $agentInput = null, mixed $boundInput = null): mixed
    {
        $executionMethod = $this->executionMethod();
        $arguments = [];

        foreach ($executionMethod->getParameters() as $parameter) {

            $parameterClass = $this->parameterClassName($parameter);

            if ($parameterClass !== null && is_a($parameterClass, ToolInputData::class, true)) {

                $arguments[] = $agentInput;

                continue;

            }

            if ($parameterClass !== null && is_a($parameterClass, ToolBoundInputData::class, true)) {

                $arguments[] = $boundInput;

                continue;

            }

            throw new RuntimeException(sprintf(
                'Tool `%s` has unsupported execution parameter `%s`. Use %s or %s implementations only.',
                $this->name(),
                $parameter->getName(),
                ToolInputData::class,
                ToolBoundInputData::class,
            ));

        }

        $result = $this->{$executionMethod->getName()}(...$arguments);

        return $this->normalizeExecutionResult($result);
    }

    protected function success(array $payload): WorkflowToolResult
    {
        return WorkflowToolResult::success($payload);
    }

    protected function fail(string $reason, array $context = []): WorkflowToolResult
    {
        return WorkflowToolResult::fail($reason, $context);
    }

    private function normalizeExecutionResult(mixed $result): mixed
    {
        if ($result instanceof WorkflowToolResult) {

            if (!$result->isSuccess()) {
                throw new RuntimeException((string) $result->reason());
            }

            return $result->payload ?? [];

        }

        if ($result instanceof BaseData && method_exists($result, 'toArray')) {
            return $result->toArray();
        }

        return $result;
    }
}
