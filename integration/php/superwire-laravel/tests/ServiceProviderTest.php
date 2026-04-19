<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Tests;

use Superwire\Contracts\Contracts\DriverRegistryInterface;
use Superwire\Contracts\Contracts\WorkflowRunnerInterface;

final class ServiceProviderTest extends TestCase
{
    public function testItRegistersCoreContractsAndPrismDriver(): void
    {
        $driverRegistry = $this->app->make(DriverRegistryInterface::class);
        $workflowRunner = $this->app->make(WorkflowRunnerInterface::class);

        $this->assertInstanceOf(DriverRegistryInterface::class, $driverRegistry);
        $this->assertInstanceOf(WorkflowRunnerInterface::class, $workflowRunner);
        $this->assertTrue($driverRegistry->has('prism'));
    }
}
