<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Tests\Unit;

use stdClass;
use Superwire\Laravel\Contracts\Tool;
use Superwire\Laravel\Contracts\ToolBoundInputData;
use Superwire\Laravel\Contracts\ToolInputData;
use Superwire\Laravel\Exceptions\InvalidToolClassException;
use Superwire\Laravel\Execution\Compiler\ToolClassValidator;
use Superwire\Laravel\Tests\TestCase;
use Swaggest\JsonSchema\Schema;

final class ToolClassValidatorTest extends TestCase
{
    public function testValidatesToolClasses(): void
    {
        $toolClassValidator = new ToolClassValidator();

        $validatedToolClasses = $toolClassValidator->validate([ ToolClassValidatorTestTool::class ]);

        $this->assertSame([ ToolClassValidatorTestTool::class ], $validatedToolClasses);
    }

    public function testThrowsForInvalidToolClass(): void
    {
        $toolClassValidator = new ToolClassValidator();

        $this->expectException(InvalidToolClassException::class);

        $toolClassValidator->validate([ stdClass::class ]);
    }
}

final class ToolClassValidatorTestTool implements Tool
{
    public static function name(): string
    {
        return 'validator_test_tool';
    }

    public static function description(): string
    {
        return 'Validator test tool';
    }

    public static function endpointName(): string
    {
        return 'validator_test_tool';
    }

    public static function agentInputClass(): string
    {
        return ToolClassValidatorTestAgentInput::class;
    }

    public static function boundInputClass(): string
    {
        return ToolClassValidatorTestBoundInput::class;
    }

    public static function outputClass(): string
    {
        return ToolClassValidatorTestOutput::class;
    }

    public static function inputSchema(): Schema
    {
        return Schema::object();
    }

    public static function boundInputSchema(): Schema
    {
        return Schema::object();
    }

    public static function outputSchema(): Schema
    {
        return Schema::object();
    }

    public static function resolveAgentInput(array $agentInputPayload): ToolInputData
    {
        return new ToolClassValidatorTestAgentInput();
    }

    public static function resolveBoundInput(array $boundInputPayload): ToolBoundInputData
    {
        return new ToolClassValidatorTestBoundInput();
    }

    public function execute(ToolInputData $agentInput, ToolBoundInputData $boundInput): array
    {
        return [];
    }
}

final class ToolClassValidatorTestAgentInput implements ToolInputData
{
}

final class ToolClassValidatorTestBoundInput implements ToolBoundInputData
{
}

final class ToolClassValidatorTestOutput
{
}
