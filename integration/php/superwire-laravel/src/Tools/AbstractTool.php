<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Tools;

use Illuminate\Support\Str;
use Prism\Prism\Schema\RawSchema;
use Prism\Prism\Tool;
use RuntimeException;
use Superwire\Laravel\Contracts\BoundInput;
use Superwire\Laravel\Contracts\ToolInput;
use Superwire\Laravel\Support\JsonSchemaFactory;
use Superwire\Laravel\Tools\Concerns\InfersToolInputSchemas;
use Superwire\Laravel\Tools\Concerns\ReflectsToolSignature;
use Superwire\Laravel\Tools\Concerns\ResolvesToolDescriptions;

abstract class AbstractTool implements WorkflowTool
{
    use InfersToolInputSchemas;
    use ReflectsToolSignature;
    use ResolvesToolDescriptions;

    public static function name(): string
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
            ->as(static::name())
            ->for(static::description());

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

    public function execute(mixed $agentInput = null, mixed $boundInput = null): array
    {
        $executionMethod = static::executionMethodReflection();
        $arguments = [];

        foreach ($executionMethod->getParameters() as $parameter) {

            $parameterClass = static::parameterClassFromReflection($parameter);

            if ($parameterClass !== null && is_a($parameterClass, ToolInput::class, true)) {

                $arguments[] = $agentInput;

                continue;

            }

            if ($parameterClass !== null && is_a($parameterClass, BoundInput::class, true)) {

                $arguments[] = $boundInput;

                continue;

            }

            throw new RuntimeException(sprintf(
                'Tool `%s` has unsupported execution parameter `%s`. Use %s or %s implementations only.',
                static::name(),
                $parameter->getName(),
                ToolInput::class,
                BoundInput::class,
            ));

        }

        $result = $this->{$executionMethod->getName()}(...$arguments);

        if (!is_array($result)) {
            throw new RuntimeException(sprintf('Tool `%s` must return an array from handle().', static::class));
        }

        return $result;
    }
}
