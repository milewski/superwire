<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Tests\Unit;

use Illuminate\Http\Request;
use Superwire\Laravel\Execution\ToolCompiler;
use Superwire\Laravel\Execution\WorkflowExecutor;
use Superwire\Laravel\Http\Controllers\InternalToolController;
use Superwire\Laravel\Support\OutputMapper;
use Superwire\Laravel\Support\ToolRegistry;
use Superwire\Laravel\Tests\TestCase;

final class SuperwireServiceProviderTest extends TestCase
{
    public function testRegistersCoreServicesInContainer(): void
    {
        $this->assertInstanceOf(WorkflowExecutor::class, app(WorkflowExecutor::class));
        $this->assertInstanceOf(ToolCompiler::class, app(ToolCompiler::class));
        $this->assertInstanceOf(OutputMapper::class, app(OutputMapper::class));
        $this->assertInstanceOf(ToolRegistry::class, app(ToolRegistry::class));
    }

    public function testLoadsInternalToolRoute(): void
    {
        $registeredRoutes = app('router')->getRoutes();
        $matchedRoute = $registeredRoutes->match(
            Request::create('/superwire/tools/echo_tool/execute', 'POST'),
        );

        $this->assertSame(InternalToolController::class, $matchedRoute->getActionName());
    }
}
