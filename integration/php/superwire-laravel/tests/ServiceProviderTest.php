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

        self::assertInstanceOf(DriverRegistryInterface::class, $driverRegistry);
        self::assertInstanceOf(WorkflowRunnerInterface::class, $workflowRunner);
        self::assertTrue($driverRegistry->has('prism'));
    }
}
