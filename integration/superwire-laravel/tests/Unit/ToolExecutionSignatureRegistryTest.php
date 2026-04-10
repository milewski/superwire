<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Tests\Unit;

use Superwire\Laravel\Tests\Fixtures\EchoTool;
use Superwire\Laravel\Tests\Fixtures\EchoToolAgentInput;
use Superwire\Laravel\Tests\Fixtures\EchoToolBoundInput;
use Superwire\Laravel\Tests\Fixtures\EchoToolOutput;
use Superwire\Laravel\Tests\TestCase;
use Superwire\Laravel\Tools\Execution\ToolExecutionSignature;
use Superwire\Laravel\Tools\Execution\ToolExecutionSignatureRegistry;
use Superwire\Laravel\Tools\Execution\ToolHandleParameter;

final class ToolExecutionSignatureRegistryTest extends TestCase
{
    public function testStoresAndResolvesSignatureByToolClass(): void
    {
        $executionSignatureRegistry = new ToolExecutionSignatureRegistry();
        $executionSignature = $this->echoToolExecutionSignature();

        $executionSignatureRegistry->set(EchoTool::class, $executionSignature);

        $this->assertTrue($executionSignatureRegistry->has(EchoTool::class));
        $this->assertSame($executionSignature, $executionSignatureRegistry->get(EchoTool::class));
    }

    public function testRememberBuildsSignatureOnlyOncePerToolClass(): void
    {
        $executionSignatureRegistry = new ToolExecutionSignatureRegistry();
        $resolutionCount = 0;

        $firstResolution = $executionSignatureRegistry->remember(EchoTool::class, function () use (&$resolutionCount): ToolExecutionSignature {
            $resolutionCount++;

            return $this->echoToolExecutionSignature();
        });

        $secondResolution = $executionSignatureRegistry->remember(EchoTool::class, function () use (&$resolutionCount): ToolExecutionSignature {
            $resolutionCount++;

            return $this->echoToolExecutionSignature();
        });

        $this->assertSame($firstResolution, $secondResolution);
        $this->assertSame(1, $resolutionCount);
    }

    public function testReturnsNullForUnknownToolClass(): void
    {
        $executionSignatureRegistry = new ToolExecutionSignatureRegistry();

        $this->assertFalse($executionSignatureRegistry->has(EchoTool::class));
        $this->assertNull($executionSignatureRegistry->get(EchoTool::class));
    }

    private function echoToolExecutionSignature(): ToolExecutionSignature
    {
        return new ToolExecutionSignature(
            agentInputClass: EchoToolAgentInput::class,
            boundInputClass: EchoToolBoundInput::class,
            outputClass: EchoToolOutput::class,
            handleParameters: [
                ToolHandleParameter::agentInput(EchoToolAgentInput::class),
                ToolHandleParameter::boundInput(EchoToolBoundInput::class),
            ],
        );
    }
}
