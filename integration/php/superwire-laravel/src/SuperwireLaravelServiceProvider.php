<?php

declare(strict_types = 1);

namespace Superwire\Laravel;

use Illuminate\Support\ServiceProvider;
use Superwire\Contracts\Contracts\DriverRegistryInterface;
use Superwire\Contracts\Contracts\WorkflowRunnerInterface;
use Superwire\Contracts\Support\LoopAgentDriver;
use Superwire\Laravel\Driver\PrismAgentDriver;
use Superwire\Laravel\Support\LaravelDriverRegistry;
use Superwire\Laravel\Support\LaravelWorkflowRunner;

final class SuperwireLaravelServiceProvider extends ServiceProvider
{
    public function register(): void
    {
        $this->app->singleton(DriverRegistryInterface::class, static fn (): DriverRegistryInterface => new LaravelDriverRegistry());

        $this->app->singleton(WorkflowRunnerInterface::class, static function ($application): WorkflowRunnerInterface {
            return new LaravelWorkflowRunner($application->make(DriverRegistryInterface::class));
        });
    }

    public function boot(): void
    {
        $driverRegistry = $this->app->make(DriverRegistryInterface::class);

        if ($driverRegistry->has('prism')) {
            return;
        }

        $driverRegistry->register('prism', new LoopAgentDriver(new PrismAgentDriver()));
    }
}
