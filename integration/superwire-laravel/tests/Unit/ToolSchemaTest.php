<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Tests\Unit;

use PHPUnit\Framework\AssertionFailedError;
use Superwire\Laravel\Tests\Fixtures\EchoTool;
use Superwire\Laravel\Tests\Fixtures\EchoToolAgentInput;
use Superwire\Laravel\Tests\Fixtures\EchoToolBoundInput;
use Superwire\Laravel\Tests\TestCase;
use Superwire\Laravel\Tools\Data\WeatherAgentInput;
use Superwire\Laravel\Tools\Data\WeatherBoundInput;
use Superwire\Laravel\Tools\Data\WeatherOutput;
use Superwire\Laravel\Tools\WeatherTool;

final class ToolSchemaTest extends TestCase
{
    public function testWeatherToolOutputPayloadMatchesSchema(): void
    {
        $this->assertOutputMatchesSchema(WeatherTool::class, [
            'city' => 'Lisbon',
            'summary' => 'Clear +18C',
            'source' => 'wttr.in via laravel package',
        ]);
    }

    public function testWeatherToolInfersClassesFromHandleSignature(): void
    {
        $this->assertSame(WeatherAgentInput::class, WeatherTool::agentInputClass());
        $this->assertSame(WeatherBoundInput::class, WeatherTool::boundInputClass());
        $this->assertSame(WeatherOutput::class, WeatherTool::outputClass());
    }

    public function testToolExecutionUsesResolvedTypedInputContracts(): void
    {
        $resolvedAgentInput = EchoTool::resolveAgentInput([
            'city' => 'Lisbon',
        ]);

        $resolvedBoundInput = EchoTool::resolveBoundInput([
            'units' => 'metric',
        ]);

        $this->assertInstanceOf(EchoToolAgentInput::class, $resolvedAgentInput);
        $this->assertInstanceOf(EchoToolBoundInput::class, $resolvedBoundInput);

        $toolOutput = (new EchoTool())->execute($resolvedAgentInput, $resolvedBoundInput);

        $this->assertSame(
            expected: [
                'agent_input' => [ 'city' => 'Lisbon' ],
                'bound_input' => [ 'units' => 'metric' ],
            ],
            actual: $toolOutput,
        );
    }

    public function testWeatherToolOutputSchemaFailsOnUnexpectedProperty(): void
    {
        $this->expectException(AssertionFailedError::class);

        $this->assertOutputMatchesSchema(WeatherTool::class, [
            'city' => 'Lisbon',
            'summary' => 'Clear +18C',
            'source' => 'wttr.in via laravel package',
            'unexpected' => 'extra-value',
        ]);
    }

    public function testWeatherToolOutputSchemaFailsWhenRequiredFieldIsNull(): void
    {
        $this->expectException(AssertionFailedError::class);

        $this->assertOutputMatchesSchema(WeatherTool::class, [
            'city' => 'Lisbon',
            'summary' => null,
            'source' => 'wttr.in via laravel package',
        ]);
    }

    public function testWeatherToolOutputSchemaFailsWhenRequiredFieldIsMissing(): void
    {
        $this->expectException(AssertionFailedError::class);

        $this->assertOutputMatchesSchema(WeatherTool::class, [
            'city' => 'Lisbon',
            'summary' => 'Clear +18C',
        ]);
    }

    public function testWeatherToolOutputSchemaFailsWhenFieldTypeIsWrong(): void
    {
        $this->expectException(AssertionFailedError::class);

        $this->assertOutputMatchesSchema(WeatherTool::class, [
            'city' => 123,
            'summary' => 'Clear +18C',
            'source' => 'wttr.in via laravel package',
        ]);
    }
}
